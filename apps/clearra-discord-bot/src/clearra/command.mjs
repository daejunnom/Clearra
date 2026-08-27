import { createHash, randomUUID } from "node:crypto";

import {
  normalizeRuntimeIdentity,
  runtimeIdentityMatches,
} from "../job-service/runtime-identity.mjs";
import { projectDiscordBuildV2Result } from "../discord/build-v2-result.mjs";
import { projectDiscordTypedProductResult } from "../discord/typed-product-result.mjs";

const JOB_PROTOCOL = "clearra.job.v1";
const DEFAULT_JOB_ENDPOINT = "http://127.0.0.1:8787/jobs";
const TERMINAL_JOB_STATES = new Set(["completed", "failed", "cancelled"]);

const NATIVE_COMMANDS = new Set([
  "pc",
  "build",
  "failed-queue",
  "setup",
  "setup-finder",
  "path",
  "pc-replay",
  "percent",
  "cover",
  "build-coverage",
  "build-probability",
  "damage",
  "spin-finder",
  "ren",
  "spin-structure",
  "finesse",
  "utility",
]);
const SFINDER_SEARCH_COMMANDS = new Set([
  "path",
  "chance",
  "percent",
  "minimals",
  "score",
  "score-minimals",
  "saves",
  "best-save",
  "cover",
  "setup",
  "congruent",
  "congruent-cover",
  "setup-cover",
  "cover-percent",
  "special-cover",
  "spin-cover",
  "spin",
  "score-finder",
]);
const SFINDER_COMMANDS = new Set([...SFINDER_SEARCH_COMMANDS, "verify"]);
const ALLOWED_COMMANDS = new Set([...NATIVE_COMMANDS, "sfinder"]);
const PARALLEL_SEARCH_COMMANDS = new Set([
  "pc",
  "build",
  "failed-queue",
  "setup",
  "setup-finder",
  "path",
  "pc-replay",
  "build-probability",
  "damage",
  "spin-finder",
  "ren",
  "spin-structure",
  "finesse",
]);
const REVERSE_SEARCH_COMMANDS = new Set([
  "pc",
  "pc-scenario",
  "failed-queue",
  "path",
  "pc-replay",
  "percent",
]);
const BUILD_SEARCH_COMMANDS = new Set([
  "build",
  "cover",
  "build-coverage",
  "build-probability",
  "finesse",
]);
const FORWARD_SEARCH_COMMANDS = new Set([
  "damage",
  "spin-finder",
  "ren",
]);
const STRUCTURE_SEARCH_COMMANDS = new Set([
  "spin-structure",
]);
const SETUP_SEARCH_COMMANDS = new Set(["setup", "setup-finder"]);
const SFINDER_REVERSE_SEARCH_COMMANDS = new Set([
  "path",
  "chance",
  "percent",
  "minimals",
  "score",
  "score-minimals",
  "saves",
  "best-save",
  "score-finder",
]);
const SFINDER_BUILD_SEARCH_COMMANDS = new Set([
  "cover",
  "setup",
  "congruent",
  "congruent-cover",
  "setup-cover",
  "cover-percent",
  "special-cover",
]);
const SFINDER_STRUCTURE_SEARCH_COMMANDS = new Set([
  "spin-cover",
  "spin",
]);
const DEFAULT_SEARCH_TIMEOUT_MS = 3 * 60_000;
const DEFAULT_PC_SEARCH_TIMEOUT_MS = 5 * 60_000;
const DEFAULT_LONG_SEARCH_TIMEOUT_MS = 15 * 60_000;
const DEFAULT_UTILITY_SEARCH_TIMEOUT_MS = 15 * 60_000;
const DISCORD_UTILITY_SUBCOMMANDS = new Set([
  "sequence",
  "sequence-dependencies",
  "parity",
  "fumen",
  "render",
  "to-gray",
  "mirror",
]);
const DISCORD_PC_SUBCOMMANDS = new Set([
  "path",
  "chance",
  "minimals",
  "score",
  "saves",
  "best-save",
  "score-minimals",
  "tiling",
  "failed-queue",
  "score-finder",
  "allspin-sol",
  "allspin-pres-chance",
]);
const DISCORD_SETUP_SUBCOMMANDS = new Set(["joint", "build", "pc", "score"]);
const DISCORD_SPIN_STRUCTURE_SUBCOMMANDS = new Set(["search", "cover", "guaranteed"]);
const BUILD_V2_TARGET_SUBCOMMANDS = new Set([
  "setup",
  "congruent",
  "congruent-cover",
  "setup-cover",
  "setup-cover-percent",
  "setup-cover-score",
]);
const BUILD_V2_EVALUATE_SUBCOMMANDS = new Set([
  "cover",
  "minimals",
  "score",
  "b2b-cover",
  "cover-percent",
]);
const BUILD_V2_SCORE_CAPABILITIES = new Set([
  "build.setup-cover-score",
  "build.evaluate.score",
]);
const BUILD_V2_COMMON_OPTIONS = new Set([
  "--queue",
  "--patterns",
  "--hold",
  "--no-hold",
  "--queue-knowledge",
  "--objective",
  "--rule",
  "--backend",
  "--no-backend-fallback",
]);
const BUILD_V2_FLAG_OPTIONS = new Set([
  "--no-hold",
  "--no-backend-fallback",
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
const SEARCH_TIMEOUT_CLASSES = new Set([
  "pc_reverse",
  "build_long",
  "setup_long",
  "forward_long",
  "structure_long",
  "utility_bounded",
  "diagnostic",
  "default",
]);

/**
 * Classifies a curated Clearra argv by its canonical product timeout family.
 * An explicitly routed class must agree with the argv classification; this
 * preserves registry authority without allowing a caller to select a more
 * permissive deadline for unrelated work.
 */
export function searchTimeoutClass(arguments_, routedClass = undefined) {
  const inferred = inferSearchTimeoutClass(arguments_);
  if (routedClass === undefined || routedClass === null || routedClass === "") {
    return inferred;
  }
  const normalized = normalizedTimeoutClass(routedClass);
  if (normalized !== inferred) {
    throw new Error(
      `Clearrabot timeout class '${normalized}' does not match '${inferred}' for this command.`,
    );
  }
  return normalized;
}

function inferSearchTimeoutClass(arguments_) {
  if (!Array.isArray(arguments_) || arguments_.length === 0) return "default";
  const command = normalizedSearchCommand(arguments_[0]);
  if (!command) return "default";
  if (SETUP_SEARCH_COMMANDS.has(command)) return "setup_long";
  if (REVERSE_SEARCH_COMMANDS.has(command)) return "pc_reverse";
  if (BUILD_SEARCH_COMMANDS.has(command)) return "build_long";
  if (FORWARD_SEARCH_COMMANDS.has(command)) return "forward_long";
  if (STRUCTURE_SEARCH_COMMANDS.has(command)) return "structure_long";
  if (
    command === "utility" &&
    DISCORD_UTILITY_SUBCOMMANDS.has(normalizedSearchCommand(arguments_[1]))
  ) {
    return "utility_bounded";
  }
  if (command !== "sfinder") return "default";

  const subcommand = normalizedSearchCommand(arguments_[1]);
  if (!subcommand) return "default";
  const canonical = normalizeSfinderCommand(subcommand);
  if (canonical === "verify") return "diagnostic";
  if (SFINDER_REVERSE_SEARCH_COMMANDS.has(canonical)) return "pc_reverse";
  if (SFINDER_BUILD_SEARCH_COMMANDS.has(canonical)) return "build_long";
  if (SFINDER_STRUCTURE_SEARCH_COMMANDS.has(canonical)) return "structure_long";
  return "default";
}

export function isSetupSearchArguments(arguments_) {
  return searchTimeoutClass(arguments_) === "setup_long";
}

export function searchTimeoutMsForArguments(
  arguments_,
  policy = {},
  routedClass = undefined,
) {
  const genericTimeoutMs = positiveSearchTimeout(
    policy.searchTimeoutMs ?? policy.timeoutMs,
    DEFAULT_SEARCH_TIMEOUT_MS,
  );
  const hasGenericOverride =
    policy.searchTimeoutMs !== undefined || policy.timeoutMs !== undefined;
  const class_ = searchTimeoutClass(arguments_, routedClass);
  const classDefault = hasGenericOverride
    ? genericTimeoutMs
    : class_ === "pc_reverse"
      ? DEFAULT_PC_SEARCH_TIMEOUT_MS
      : class_.endsWith("_long")
        ? DEFAULT_LONG_SEARCH_TIMEOUT_MS
        : class_ === "utility_bounded"
          ? DEFAULT_UTILITY_SEARCH_TIMEOUT_MS
        : DEFAULT_SEARCH_TIMEOUT_MS;
  const configured = ({
    pc_reverse: policy.pcSearchTimeoutMs ?? policy.reverseSearchTimeoutMs,
    build_long: policy.buildSearchTimeoutMs ?? policy.forwardSearchTimeoutMs,
    setup_long: policy.setupSearchTimeoutMs ?? policy.forwardSearchTimeoutMs,
    forward_long: policy.forwardSearchTimeoutMs,
    structure_long:
      policy.structureSearchTimeoutMs ?? policy.forwardSearchTimeoutMs,
    utility_bounded: policy.utilitySearchTimeoutMs,
    diagnostic: policy.diagnosticTimeoutMs,
    default: undefined,
  })[class_];
  return positiveSearchTimeout(configured, classDefault);
}

function normalizedTimeoutClass(value) {
  if (typeof value !== "string") {
    throw new Error("Clearrabot received an invalid search timeout class.");
  }
  const normalized = value.trim().toLowerCase().replaceAll("-", "_");
  if (!SEARCH_TIMEOUT_CLASSES.has(normalized)) {
    throw new Error("Clearrabot received an invalid search timeout class.");
  }
  return normalized;
}

function normalizedSearchCommand(value) {
  if (typeof value !== "string") return null;
  const normalized = value.trim().toLowerCase().replaceAll("_", "-");
  return normalized.length > 0 ? normalized : null;
}

function positiveSearchTimeout(value, fallback) {
  if (value === undefined || value === null || value === "") return fallback;
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed < 1) {
    throw new Error("Clearrabot received an invalid search timeout policy.");
  }
  return parsed;
}

/**
 * Returns the canonical, input-free command path that is safe to retain in an
 * operational record. The array form accepts a prepared Clearra argv; the
 * string form accepts only an already reduced command path.
 */
export function canonicalClearraOperationalCommand(value) {
  const pathInput = typeof value === "string";
  const tokens = Array.isArray(value)
    ? value
    : pathInput
      ? value.split(".")
      : [];
  const command = normalizedOperationalPart(tokens[0]);
  if (!command) return null;
  if (command === "finesse") {
    if (pathInput && tokens.length !== 2) return null;
    const subcommand = normalizedOperationalPart(tokens[1]);
    return subcommand === "search" || subcommand === "score"
      ? `finesse.${subcommand}`
      : null;
  }
  if (command === "utility") {
    if (pathInput && tokens.length !== 2) return null;
    const subcommand = normalizedOperationalPart(tokens[1]);
    return DISCORD_UTILITY_SUBCOMMANDS.has(subcommand)
      ? `utility.${subcommand}`
      : null;
  }
  if (command === "build") {
    const subcommand = normalizedOperationalPart(tokens[1]);
    if (subcommand === "cover" || BUILD_V2_TARGET_SUBCOMMANDS.has(subcommand)) {
      return !pathInput || tokens.length === 2 ? `build.${subcommand}` : null;
    }
    if (subcommand !== "evaluate") return null;
    const evaluation = normalizedOperationalPart(tokens[2]);
    return BUILD_V2_EVALUATE_SUBCOMMANDS.has(evaluation) &&
      (!pathInput || tokens.length === 3)
      ? `build.evaluate.${evaluation}`
      : null;
  }
  if (command === "pc") {
    const subcommand = normalizedOperationalPart(tokens[1]);
    if (DISCORD_PC_SUBCOMMANDS.has(subcommand)) {
      return !pathInput || tokens.length === 2 ? `pc.${subcommand}` : null;
    }
    return !pathInput || tokens.length === 1 ? command : null;
  }
  if (command === "setup") {
    const subcommand = normalizedOperationalPart(tokens[1]);
    if (DISCORD_SETUP_SUBCOMMANDS.has(subcommand)) {
      return !pathInput || tokens.length === 2 ? `setup.${subcommand}` : null;
    }
    return !pathInput || tokens.length === 1 ? command : null;
  }
  if (command === "spin-structure") {
    const subcommand = normalizedOperationalPart(tokens[1]);
    if (DISCORD_SPIN_STRUCTURE_SUBCOMMANDS.has(subcommand)) {
      return !pathInput || tokens.length === 2
        ? `spin-structure.${subcommand}`
        : null;
    }
    return !pathInput || tokens.length === 1 ? command : null;
  }
  if (NATIVE_COMMANDS.has(command)) {
    return !pathInput || tokens.length === 1 ? command : null;
  }
  if (command !== "sfinder" || (pathInput && tokens.length !== 2)) return null;
  const subcommand = normalizedOperationalPart(tokens[1]);
  if (!subcommand) return null;
  const canonical = normalizeSfinderCommand(subcommand);
  return SFINDER_COMMANDS.has(canonical) ? `sfinder.${canonical}` : null;
}
const FILE_OPTIONS = new Set([
  "--fixture",
  "--file",
  "--input",
  "--output",
  "--output-base",
  "--template-file",
  "--field-path",
  "--patterns-path",
  "--log-path",
  "--wgsl",
  "--kick-profile-json",
  "--document",
  "-fp",
  "-pp",
  "-lp",
  "-o",
]);
const CONTROLLED_OPTIONS = new Map([
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
const FORBIDDEN_TIE_OPTIONS = new Set([
  "--ties",
  "--tie-snapshot",
  "--tie-cursor",
]);
const FORBIDDEN_ALTERNATIVE_RESULT_KEYS = new Set([
  "alternative",
  "alternatives",
  "alternative_count",
  "alternative_index",
  "known_alternative_count",
  "total_alternative_count",
  "portfolio_alternative_page",
  "product_result_payload",
  "score_informational_attack_basis",
  "score_pattern_winner_contract",
  "score_pattern_winner_equality",
  "score_pattern_winner_ordering",
  "score_pattern_winners",
  "tie",
  "ties",
  "tie_cursor",
  "tie_metadata",
  "tie_snapshot",
]);

/** Discord deliberately has no alternative-page authority. */
export function assertDiscordNoTieArguments(arguments_) {
  for (const argument of arguments_) {
    if (typeof argument !== "string") continue;
    const normalized = argument.trim().toLowerCase();
    const equalsIndex = normalized.indexOf("=");
    const option = equalsIndex < 0 ? normalized : normalized.slice(0, equalsIndex);
    if (FORBIDDEN_TIE_OPTIONS.has(option)) {
      throw new Error("Discord does not expose alternative-result paging options.");
    }
  }
}

/**
 * Rejects an engine/job response that tries to widen Discord's canonical-only
 * result surface. Normal product families and lists remain valid; only the
 * explicit alternative/tie contracts are forbidden.
 */
export function assertDiscordCanonicalOnlyResult(result) {
  const stdout = typeof result?.stdout === "string" ? result.stdout.trim() : "";
  if (!stdout) return result;
  let payload;
  try {
    payload = JSON.parse(stdout);
  } catch {
    return result;
  }
  const buildProjection = projectDiscordBuildV2Result(payload);
  if (buildProjection !== null) payload = buildProjection;
  const typedProjection = buildProjection === null
    ? projectDiscordTypedProductResult(payload)
    : null;
  if (typedProjection !== null) payload = typedProjection;
  if (containsForbiddenAlternativeMetadata(payload)) {
    throw new Error("Clearra returned alternative-result metadata that Discord cannot expose.");
  }
  const projection = buildProjection ?? typedProjection;
  if (projection === null) return result;
  return Object.freeze({
    ...result,
    stdout: JSON.stringify(projection),
  });
}

export function parseClearraMessage(content, prefix = "!", execution = {}) {
  // The catalog-aware text ingress owns every executable text alias. Keeping
  // this legacy raw parser fail-closed prevents older imports from restoring
  // the removed `!clearra ...` or noncatalog `!sfinder ...` escape hatch.
  void content;
  void prefix;
  void execution;
  return null;
}

export function prepareClearraArguments(tokens, execution = {}) {
  if (!Array.isArray(tokens) || tokens.length === 0) {
    throw new Error("Enter a Clearra command.");
  }
  if (tokens.length > 256) throw new Error("The command has too many arguments.");
  assertDiscordNoTieArguments(tokens);
  const command = tokens[0].toLowerCase();
  if (!ALLOWED_COMMANDS.has(command)) {
    throw new Error(
      "Discord supports curated Clearra PC, build, setup, coverage, forward, and sfinder searches.",
    );
  }
  const sfinderCommand = command === "sfinder"
    ? validateSfinderCommand(tokens[1])
    : null;
  const buildV2Contract = command === "build" ? validateBuildV2Command(tokens) : null;
  if (command === "finesse" && !["search", "score"].includes(tokens[1]?.toLowerCase())) {
    throw new Error("Discord finesse calculations require a search or score subcommand.");
  }
  if (
    command === "utility" &&
    !DISCORD_UTILITY_SUBCOMMANDS.has(normalizedSearchCommand(tokens[1]))
  ) {
    throw new Error("Discord utility execution requires a registered typed-document subcommand.");
  }

  const output = [command];
  for (let index = 1; index < tokens.length; index += 1) {
    const token = tokens[index];
    const normalizedToken = token.toLowerCase();
    const equalsIndex = normalizedToken.indexOf("=");
    const optionName = equalsIndex < 0
      ? normalizedToken
      : normalizedToken.slice(0, equalsIndex);
    if (command === "build") {
      assertBuildV2ExecutionOption(
        buildV2Contract,
        tokens,
        index,
        optionName,
        equalsIndex,
      );
    }
    const inlineOperationDocument = optionName === "--document" && (
      command === "utility" &&
        DISCORD_UTILITY_SUBCOMMANDS.has(normalizedSearchCommand(tokens[1])) ||
      command === "setup" &&
        normalizedSearchCommand(tokens[1]) === "score"
    );
    if (FILE_OPTIONS.has(optionName) && !inlineOperationDocument) {
      throw new Error("File and custom-code inputs are not available through Discord.");
    }
    const controlledWidth = CONTROLLED_OPTIONS.get(optionName);
    if (controlledWidth !== undefined) {
      if (equalsIndex < 0) index += controlledWidth;
      continue;
    }
    output.push(token);
  }

  const canonicalPcSubcommand = command === "pc" &&
    DISCORD_PC_SUBCOMMANDS.has(normalizedSearchCommand(tokens[1]));
  if (command === "failed-queue" || command === "pc" && !canonicalPcSubcommand) {
    output.push("--no-tablebase", "--no-build-dependency-dag");
  } else if (
    command === "setup-finder" ||
    command === "setup" && normalizedSearchCommand(tokens[1]) !== "score"
  ) {
    output.push("--no-tablebase");
  }
  const fixedSingleWorkerPcScore = command === "pc" &&
    ["score", "score-minimals", "score-finder"].includes(normalizedSearchCommand(tokens[1]));
  const parallelSearch = !fixedSingleWorkerPcScore && (
    PARALLEL_SEARCH_COMMANDS.has(command) ||
    (command === "sfinder" && SFINDER_SEARCH_COMMANDS.has(sfinderCommand))
  );
  if (parallelSearch) {
    if (execution.workers !== undefined) {
      const workers = Number(execution.workers);
      if (!Number.isSafeInteger(workers) || workers < 1) {
        throw new Error("Clearrabot received an invalid search worker allocation.");
      }
      if (execution.logicalProcessors !== undefined) {
        const logicalProcessors = Number(execution.logicalProcessors);
        if (!Number.isSafeInteger(logicalProcessors) || logicalProcessors < 1) {
          throw new Error("Clearrabot received an invalid logical processor limit.");
        }
        if (workers > logicalProcessors) {
          throw new Error(
            `Clearrabot worker allocation exceeds the hard limit of ${logicalProcessors} logical processors.`,
          );
        }
      }
      output.push("--auto-workers", String(workers));
    }
    if (execution.useAllLogicalProcessors) {
      output.push("--use-all-cpu-threads");
    }
  }
  const outputFormat = execution.outputFormat ?? "text";
  if (outputFormat !== "text" && outputFormat !== "json") {
    throw new Error("Clearrabot received an invalid output format policy.");
  }
  output.push("--format", outputFormat);
  if (execution.includeSolutionData === true && command !== "utility") {
    output.push("--include-solution-data");
  }
  return output;
}

function validateBuildV2Command(tokens) {
  const subcommand = normalizedSearchCommand(tokens[1]);
  let capabilityId;
  let optionStart;
  let sourceOptions;
  if (subcommand === "cover") {
    capabilityId = "build.cover";
    optionStart = 2;
    sourceOptions = ["--base-mask", "--target-mask", "--height", "--source-pieces"];
  } else if (BUILD_V2_TARGET_SUBCOMMANDS.has(subcommand)) {
    capabilityId = `build.${subcommand}`;
    optionStart = 2;
    sourceOptions = ["--target-format", "--target-document"];
  } else if (
    subcommand === "evaluate" &&
    BUILD_V2_EVALUATE_SUBCOMMANDS.has(normalizedSearchCommand(tokens[2]))
  ) {
    capabilityId = `build.evaluate.${normalizedSearchCommand(tokens[2])}`;
    optionStart = 3;
    sourceOptions = ["--solution-format", "--solution-document"];
  } else {
    throw new Error("Discord build execution requires one registered Build v2 capability path.");
  }

  const allowedOptions = new Set([...BUILD_V2_COMMON_OPTIONS, ...sourceOptions]);
  if (BUILD_V2_SCORE_CAPABILITIES.has(capabilityId)) {
    allowedOptions.add("--score-profile");
    allowedOptions.add("--initial-b2b");
  }
  const contract = Object.freeze({
    capabilityId,
    optionStart,
    sourceOptions: Object.freeze([...sourceOptions]),
    allowedOptions,
  });
  assertClosedBuildV2Execution(tokens, contract);
  return contract;
}

function assertBuildV2ExecutionOption(contract, tokens, index, optionName, equalsIndex) {
  if ([
    "--max-memory-mib",
    "--max-memory",
    "--memory-budget-mb",
    "--memory-budget",
  ].includes(optionName)) {
    throw new Error("Discord Build v2 has no max-memory request authority.");
  }
  if (optionName === "--allow-backend-fallback") {
    throw new Error("Discord Build v2 is CPU-only and forbids backend fallback.");
  }
  if (index < contract.optionStart || !optionName.startsWith("--")) return;
  if (!contract.allowedOptions.has(optionName)) {
    throw new Error(
      `${contract.capabilityId} does not expose the Build v2 option '${optionName}'.`,
    );
  }
  if (optionName !== "--backend") return;
  const value = equalsIndex < 0
    ? String(tokens[index + 1] ?? "").trim().toLowerCase()
    : String(tokens[index]).slice(equalsIndex + 1).trim().toLowerCase();
  if (value !== "cpu") {
    throw new Error("Discord Build v2 is CPU-only.");
  }
}

function assertClosedBuildV2Execution(tokens, contract) {
  const values = new Map();
  for (let index = contract.optionStart; index < tokens.length; index += 1) {
    const token = String(tokens[index] ?? "");
    const equalsIndex = token.indexOf("=");
    const optionName = (equalsIndex < 0 ? token : token.slice(0, equalsIndex))
      .trim()
      .toLowerCase();
    assertBuildV2ExecutionOption(contract, tokens, index, optionName, equalsIndex);
    if (!optionName.startsWith("--")) {
      throw new Error(`${contract.capabilityId} does not accept positional Build v2 inputs.`);
    }
    if (values.has(optionName)) {
      throw new Error(`${contract.capabilityId} received '${optionName}' more than once.`);
    }
    if (BUILD_V2_FLAG_OPTIONS.has(optionName)) {
      if (equalsIndex >= 0) {
        throw new Error(`${optionName} is a flag and does not accept a value.`);
      }
      values.set(optionName, true);
      continue;
    }
    const value = equalsIndex >= 0
      ? token.slice(equalsIndex + 1)
      : String(tokens[++index] ?? "");
    if (!value || value.startsWith("--")) {
      throw new Error(`${optionName} requires one explicit value.`);
    }
    values.set(optionName, value);
  }

  for (const sourceOption of contract.sourceOptions.filter(
    (option) => option !== "--source-pieces",
  )) {
    if (!values.has(sourceOption)) {
      throw new Error(`${contract.capabilityId} requires '${sourceOption}'.`);
    }
  }
  if (values.has("--queue") === values.has("--patterns")) {
    throw new Error(`${contract.capabilityId} requires exactly one of --queue or --patterns.`);
  }
  if (values.has("--hold") === values.has("--no-hold")) {
    throw new Error(`${contract.capabilityId} requires exactly one explicit hold policy.`);
  }
  for (const required of [
    "--queue-knowledge",
    "--objective",
    "--rule",
    "--backend",
    "--no-backend-fallback",
  ]) {
    if (!values.has(required)) {
      throw new Error(`${contract.capabilityId} requires '${required}'.`);
    }
  }

  const queueKnowledge = String(values.get("--queue-knowledge")).toLowerCase();
  if (!["oracle", "visible-7"].includes(queueKnowledge)) {
    throw new Error("Discord Build v2 queue knowledge must be oracle or visible-7.");
  }
  const objective = String(values.get("--objective")).toLowerCase();
  if (!BUILD_V2_OBJECTIVES[contract.capabilityId]?.includes(objective)) {
    throw new Error(`${contract.capabilityId} does not expose objective '${objective}'.`);
  }
  const rule = String(values.get("--rule")).toLowerCase();
  if (!["srs-plus", "srs", "srs-x", "jstris-180", "no-kick"].includes(rule)) {
    throw new Error("Discord Build v2 requires one registered native rule profile.");
  }
  if (BUILD_V2_SCORE_CAPABILITIES.has(contract.capabilityId)) {
    for (const required of ["--score-profile", "--initial-b2b"]) {
      if (!values.has(required)) {
        throw new Error(`${contract.capabilityId} requires '${required}'.`);
      }
    }
    if (![
      "tetrio",
      "guideline",
      "jstris-ultra",
    ].includes(String(values.get("--score-profile")).toLowerCase())) {
      throw new Error("Discord Build v2 requires one registered score profile.");
    }
  }
}

function validateSfinderCommand(value) {
  if (typeof value !== "string" || value.length === 0) {
    throw new Error("Discord sfinder searches require a subcommand.");
  }
  const command = normalizeSfinderCommand(value);
  if (!SFINDER_COMMANDS.has(command)) {
    throw new Error(`Discord does not expose the sfinder '${command}' contract.`);
  }
  return command;
}

function normalizeSfinderCommand(value) {
  const normalized = value.trim().toLowerCase().replaceAll("_", "-");
  return ({
    bestsave: "best-save",
    bestsetup: "best-setup",
    congruentcover: "congruent-cover",
    coverpercent: "cover-percent",
    dpcfinder: "dpc-finder",
    pcsetup: "pc-setup",
    scoreminimals: "score-minimals",
    setupcover: "setup-cover",
    specialcover: "special-cover",
    spincover: "spin-cover",
  })[normalized] ?? normalized;
}

function normalizedOperationalPart(value) {
  if (typeof value !== "string") return null;
  const normalized = value.trim().toLowerCase().replaceAll("_", "-");
  return /^[a-z0-9][a-z0-9-]{0,31}$/.test(normalized)
    ? normalized
    : null;
}

export function tilingOnlyRequested(arguments_) {
  if (!Array.isArray(arguments_)) return false;
  if (
    String(arguments_[0] ?? "").toLowerCase() === "pc" &&
    String(arguments_[1] ?? "").toLowerCase().replaceAll("_", "-") === "tiling"
  ) {
    return true;
  }
  for (let index = 0; index < arguments_.length; index += 1) {
    const token = String(arguments_[index]).toLowerCase();
    if (token === "--tiling-only") return true;
    if (
      token === "--objective" &&
      String(arguments_[index + 1] ?? "").toLowerCase() === "tiling"
    ) {
      return true;
    }
  }
  return false;
}

export function tokenizeCommand(source) {
  if (source.length > 8192) throw new Error("The command is too long.");
  const tokens = [];
  let token = "";
  let quote = null;
  let escaped = false;

  for (const character of source) {
    if (escaped) {
      token += character;
      escaped = false;
      continue;
    }
    if (character === "\\") {
      escaped = true;
      continue;
    }
    if (quote) {
      if (character === quote) quote = null;
      else token += character;
      continue;
    }
    if (character === "'" || character === '"') {
      quote = character;
      continue;
    }
    if (/\s/.test(character)) {
      if (token) {
        tokens.push(token);
        token = "";
      }
      continue;
    }
    token += character;
  }
  if (escaped) token += "\\";
  if (quote) throw new Error("The command contains an unterminated quote.");
  if (token) tokens.push(token);
  return tokens;
}

export class ClearraJobExecutor {
  constructor(options = {}) {
    this.endpoint = normalizeJobEndpoint(
      options.endpoint ?? DEFAULT_JOB_ENDPOINT,
    );
    this.authorizationToken = options.authorizationToken ?? null;
    this.expectedRuntimeIdentity = options.expectedRuntimeIdentity
      ? normalizeRuntimeIdentity(options.expectedRuntimeIdentity)
      : null;
    if (!this.authorizationToken && !isLoopbackHostname(this.endpoint.hostname)) {
      throw new Error("A remote Clearra job endpoint requires an authorization token.");
    }
    const legacyTimeoutMs = options.timeoutMs === undefined
      ? undefined
      : positiveExecutorOption(options.timeoutMs, DEFAULT_SEARCH_TIMEOUT_MS);
    this.searchTimeoutMs = positiveExecutorOption(
      options.searchTimeoutMs ?? legacyTimeoutMs,
      DEFAULT_SEARCH_TIMEOUT_MS,
    );
    this.pcSearchTimeoutMs = positiveExecutorOption(
      options.pcSearchTimeoutMs ?? options.reverseSearchTimeoutMs ?? legacyTimeoutMs,
      legacyTimeoutMs ?? DEFAULT_PC_SEARCH_TIMEOUT_MS,
    );
    // Retain the old property as a read-only compatibility projection for
    // callers that have not yet moved to the canonical PC family name.
    this.reverseSearchTimeoutMs = this.pcSearchTimeoutMs;
    this.buildSearchTimeoutMs = positiveExecutorOption(
      options.buildSearchTimeoutMs ?? options.forwardSearchTimeoutMs ?? legacyTimeoutMs,
      legacyTimeoutMs ?? DEFAULT_LONG_SEARCH_TIMEOUT_MS,
    );
    this.setupSearchTimeoutMs = positiveExecutorOption(
      options.setupSearchTimeoutMs ?? options.forwardSearchTimeoutMs ?? legacyTimeoutMs,
      legacyTimeoutMs ?? DEFAULT_LONG_SEARCH_TIMEOUT_MS,
    );
    this.forwardSearchTimeoutMs = positiveExecutorOption(
      options.forwardSearchTimeoutMs ?? legacyTimeoutMs,
      legacyTimeoutMs ?? DEFAULT_LONG_SEARCH_TIMEOUT_MS,
    );
    this.structureSearchTimeoutMs = positiveExecutorOption(
      options.structureSearchTimeoutMs ?? options.forwardSearchTimeoutMs ?? legacyTimeoutMs,
      legacyTimeoutMs ?? DEFAULT_LONG_SEARCH_TIMEOUT_MS,
    );
    this.utilitySearchTimeoutMs = positiveExecutorOption(
      options.utilitySearchTimeoutMs,
      DEFAULT_UTILITY_SEARCH_TIMEOUT_MS,
    );
    this.diagnosticTimeoutMs = positiveExecutorOption(
      options.diagnosticTimeoutMs ?? legacyTimeoutMs,
      legacyTimeoutMs ?? DEFAULT_SEARCH_TIMEOUT_MS,
    );
    this.timeoutMs = Math.max(
      this.searchTimeoutMs,
      this.pcSearchTimeoutMs,
      this.buildSearchTimeoutMs,
      this.setupSearchTimeoutMs,
      this.forwardSearchTimeoutMs,
      this.structureSearchTimeoutMs,
      this.utilitySearchTimeoutMs,
      this.diagnosticTimeoutMs,
    );
    this.maxOutputBytes = positiveExecutorOption(
      options.maxOutputBytes,
      4 * 1024 * 1024,
    );
    this.maxArtifactBytes = positiveExecutorOption(
      options.maxArtifactBytes,
      24 * 1024 * 1024,
    );
    this.pollIntervalMs = positiveExecutorOption(options.pollIntervalMs, 250);
    this.cancelTimeoutMs = positiveExecutorOption(options.cancelTimeoutMs, 2_000);
    this.fetch = options.fetch ?? globalThis.fetch?.bind(globalThis);
    this.createJobId = options.createJobId ?? randomUUID;
    this.now = options.now ?? Date.now;
    if (typeof this.fetch !== "function") {
      throw new Error("Clearrabot requires an HTTP fetch implementation.");
    }
  }

  async execute(arguments_, options = {}) {
    validateJobArguments(arguments_);
    const jobId = String(options.jobId ?? this.createJobId());
    if (!jobId || jobId.length > 128) {
      throw new Error("Clearrabot could not allocate a valid job ID.");
    }

    const controller = new AbortController();
    const startedAt = this.now();
    const timeoutClass = searchTimeoutClass(arguments_, options.timeoutClass);
    const commandTimeoutMs = searchTimeoutMsForArguments(
      arguments_,
      this,
      timeoutClass,
    );
    const requestedDeadlineUnixMs = options.deadlineUnixMs === undefined
      ? startedAt + commandTimeoutMs
      : validAbsoluteDeadline(options.deadlineUnixMs);
    const deadlineUnixMs = Math.min(
      requestedDeadlineUnixMs,
      startedAt + commandTimeoutMs,
    );
    const remainingMs = deadlineUnixMs - startedAt;
    if (remainingMs <= 0) {
      throw new Error("Clearra interaction deadline expired before submission.");
    }
    let timedOut = false;
    let submitted = false;
    const abort = () => controller.abort();
    const timeout = setTimeout(() => {
      timedOut = true;
      controller.abort();
    }, remainingMs);
    options.signal?.addEventListener("abort", abort, { once: true });

    try {
      if (options.signal?.aborted) controller.abort();
      submitted = true;
      let response = await this.request(this.endpoint, {
        method: "POST",
        jobId,
        signal: controller.signal,
        body: JSON.stringify({
          protocol: JOB_PROTOCOL,
          id: jobId,
          kind: "clearra.command",
          arguments: [...arguments_],
          timeoutClass,
          deadlineUnixMs,
          maxOutputBytes: this.maxOutputBytes,
          maxArtifactBytes: this.maxArtifactBytes,
          expectedRuntime: this.expectedRuntimeIdentity,
        }),
      });
      let job = await readJobResponse(
        response,
        this.maxOutputBytes,
        this.maxArtifactBytes,
      );
      validateRuntimeIdentity(job, this.expectedRuntimeIdentity);
      validateJobIdentity(job, jobId);

      while (!TERMINAL_JOB_STATES.has(job.state)) {
        if (job.state !== "running" && job.state !== "accepted") {
          throw new Error("Clearra job service returned an invalid pending state.");
        }
        await abortableDelay(this.pollIntervalMs, controller.signal);
        response = await this.request(this.jobUrl(jobId), {
          method: "GET",
          jobId,
          signal: controller.signal,
        });
        job = await readJobResponse(
          response,
          this.maxOutputBytes,
          this.maxArtifactBytes,
        );
        validateRuntimeIdentity(job, this.expectedRuntimeIdentity);
        validateJobIdentity(job, jobId);
      }

      return terminalJobResult(
        job,
        this.maxOutputBytes,
        this.maxArtifactBytes,
      );
    } catch (error) {
      if (submitted) await this.cancel(jobId);
      if (controller.signal.aborted) {
        if (timedOut) {
          throw new Error(
            `Clearrabot search exceeded the ${timeoutLabel(commandTimeoutMs)} time limit.`,
          );
        }
        throw abortError("Clearra search was cancelled.");
      }
      if (error instanceof Error) throw error;
      throw new Error("Clearra job service request failed.");
    } finally {
      clearTimeout(timeout);
      options.signal?.removeEventListener("abort", abort);
    }
  }

  async request(url, options) {
    let response;
    try {
      response = await this.fetch(url, {
        method: options.method,
        headers: this.headers(options.jobId, options.body !== undefined),
        body: options.body,
        signal: options.signal,
        cache: "no-store",
        redirect: "error",
      });
    } catch (error) {
      if (options.signal?.aborted) throw error;
      const detail = error instanceof Error ? `: ${error.message}` : "";
      throw new Error(`Clearra job service could not be reached${detail}`);
    }
    if (!response.ok) {
      const detail = await readBoundedText(response, 16 * 1024).catch(() => "");
      const suffix = detail ? `: ${detail.slice(0, 512)}` : "";
      throw new Error(
        `Clearra job service rejected the request (${response.status})${suffix}`,
      );
    }
    return response;
  }

  headers(jobId, hasBody) {
    const headers = {
      accept: "application/json",
      "idempotency-key": jobId,
    };
    if (hasBody) headers["content-type"] = "application/json";
    if (this.authorizationToken) {
      headers.authorization = `Bearer ${this.authorizationToken}`;
    }
    return headers;
  }

  jobUrl(jobId) {
    const url = new URL(this.endpoint);
    url.pathname = `${url.pathname.replace(/\/$/, "")}/${encodeURIComponent(jobId)}`;
    return url;
  }

  async cancel(jobId) {
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), this.cancelTimeoutMs);
    try {
      const response = await this.fetch(this.jobUrl(jobId), {
        method: "DELETE",
        headers: this.headers(jobId, false),
        signal: controller.signal,
        cache: "no-store",
        redirect: "error",
      });
      if (!response.ok && response.status !== 404 && response.status !== 409) {
        throw new Error(`job cancellation returned ${response.status}`);
      }
    } catch {
      // The submitted deadline remains the service-side fail-close boundary.
    } finally {
      clearTimeout(timeout);
    }
  }
}

function normalizeJobEndpoint(value) {
  let url;
  try {
    url = new URL(String(value));
  } catch {
    throw new Error("Clearrabot received an invalid Clearra job endpoint.");
  }
  if (url.protocol !== "http:" && url.protocol !== "https:") {
    throw new Error("Clearra job endpoint must use HTTP or HTTPS.");
  }
  if (url.username || url.password) {
    throw new Error("Clearra job endpoint must not contain credentials.");
  }
  if (url.protocol === "http:" && !isLoopbackHostname(url.hostname)) {
    throw new Error("Clearra job endpoint must use HTTPS unless it targets loopback.");
  }
  url.hash = "";
  return url;
}

function isLoopbackHostname(hostname) {
  const normalized = String(hostname).toLowerCase();
  return normalized === "localhost" ||
    normalized === "::1" ||
    normalized === "[::1]" ||
    /^127(?:\.\d{1,3}){3}$/.test(normalized);
}

function positiveExecutorOption(value, fallback) {
  const parsed = value ?? fallback;
  if (!Number.isSafeInteger(parsed) || parsed < 1) {
    throw new Error("Clearrabot received an invalid job executor setting.");
  }
  return parsed;
}

function validAbsoluteDeadline(value) {
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed < 1) {
    throw new Error("Clearrabot received an invalid interaction deadline.");
  }
  return parsed;
}

function validateJobArguments(arguments_) {
  if (!Array.isArray(arguments_) || arguments_.length === 0) {
    throw new Error("Clearrabot cannot submit an empty Clearra job.");
  }
  for (const argument of arguments_) {
    if (typeof argument !== "string" || argument.includes("\0")) {
      throw new Error("Clearrabot received an invalid Clearra job argument.");
    }
  }
  assertDiscordNoTieArguments(arguments_);
}

async function readJobResponse(response, maxOutputBytes, maxArtifactBytes) {
  const text = await readBoundedText(
    response,
    checkedJobResponseLimit(maxOutputBytes, maxArtifactBytes),
  );
  let value;
  try {
    value = JSON.parse(text);
  } catch {
    throw new Error("Clearra job service returned invalid JSON.");
  }
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("Clearra job service returned an invalid job response.");
  }
  if (value.protocol !== JOB_PROTOCOL) {
    throw new Error("Clearra job service protocol is not compatible with Clearrabot.");
  }
  if (typeof value.state !== "string") {
    throw new Error("Clearra job service omitted the job state.");
  }
  return value;
}

async function readBoundedText(response, maxBytes) {
  const declaredLength = Number(response.headers.get("content-length"));
  if (Number.isFinite(declaredLength) && declaredLength > maxBytes) {
    throw new Error("Clearra produced too much Discord output.");
  }
  if (!response.body) return "";

  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let received = 0;
  let text = "";
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      received += value.byteLength;
      if (received > maxBytes) {
        await reader.cancel();
        throw new Error("Clearra produced too much Discord output.");
      }
      text += decoder.decode(value, { stream: true });
    }
    text += decoder.decode();
    return text;
  } finally {
    reader.releaseLock();
  }
}

function validateJobIdentity(job, expectedId) {
  if (job.id !== expectedId) {
    throw new Error("Clearra job service returned a mismatched job ID.");
  }
}

function validateRuntimeIdentity(job, expected) {
  if (!expected) return;
  if (!runtimeIdentityMatches(job.runtime, expected)) {
    throw new Error("Clearra job service runtime identity does not match Clearrabot.");
  }
}

function terminalJobResult(job, maxOutputBytes, maxArtifactBytes) {
  if (job.state === "cancelled") {
    throw abortError("Clearra job service cancelled the search.");
  }
  if (job.state === "failed") {
    const message = typeof job.error === "string" ? job.error : "remote job failed";
    throw new Error(`Clearra job service failed the search: ${message}`);
  }
  const result = job.result;
  if (!result || typeof result !== "object" || Array.isArray(result)) {
    throw new Error("Clearra job service omitted the completed result.");
  }
  if (!Number.isSafeInteger(result.exitCode)) {
    throw new Error("Clearra job service returned an invalid exit code.");
  }
  const stdout = typeof result.stdout === "string" ? result.stdout : "";
  const stderr = typeof result.stderr === "string" ? result.stderr : "";
  if (Buffer.byteLength(stdout) + Buffer.byteLength(stderr) > maxOutputBytes) {
    throw new Error("Clearra produced too much Discord output.");
  }
  if (result.signal !== null && result.signal !== undefined && typeof result.signal !== "string") {
    throw new Error("Clearra job service returned an invalid process signal.");
  }
  const artifact = validateTransportArtifact(result.artifact, maxArtifactBytes);
  return assertDiscordCanonicalOnlyResult({
    exitCode: result.exitCode,
    signal: result.signal ?? null,
    stdout: stdout.trim(),
    stderr: stderr.trim(),
    ...(artifact ? { artifact } : {}),
  });
}

function checkedJobResponseLimit(maxOutputBytes, maxArtifactBytes) {
  const encodedArtifactBytes = Math.ceil(maxArtifactBytes / 3) * 4;
  const total = maxOutputBytes * 6 + encodedArtifactBytes + 64 * 1024;
  if (!Number.isSafeInteger(total) || total < 1) {
    throw new Error("Clearrabot job response limits are invalid.");
  }
  return total;
}

function validateTransportArtifact(value, maximumBytes) {
  if (value === undefined || value === null) return null;
  if (
    !value ||
    typeof value !== "object" ||
    Array.isArray(value) ||
    value.contract !== "clearra.discord-render-artifact.v1" ||
    !["png", "gif"].includes(value.artifactFormat) ||
    value.mediaType !== (value.artifactFormat === "png" ? "image/png" : "image/gif") ||
    typeof value.filename !== "string" ||
    value.filename.length > 128 ||
    !/^[a-z0-9][a-z0-9._-]*$/i.test(value.filename) ||
    !value.filename.toLowerCase().endsWith(`.${value.artifactFormat}`) ||
    !Number.isSafeInteger(value.byteLength) ||
    value.byteLength < 1 ||
    value.byteLength > maximumBytes ||
    typeof value.sha256 !== "string" ||
    !/^[a-f0-9]{64}$/.test(value.sha256) ||
    typeof value.bytesBase64 !== "string" ||
    value.renderExact !== true
  ) {
    throw new Error("Clearra job service returned an invalid render artifact.");
  }
  const bytes = Buffer.from(value.bytesBase64, "base64");
  const canonicalBase64 = bytes.toString("base64");
  const validSignature = value.artifactFormat === "png"
    ? bytes.length >= 8 && bytes.subarray(0, 8).equals(
        Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
      )
    : bytes.length >= 6 &&
      ["GIF87a", "GIF89a"].includes(bytes.subarray(0, 6).toString("ascii"));
  if (
    canonicalBase64 !== value.bytesBase64 ||
    bytes.length !== value.byteLength ||
    createHash("sha256").update(bytes).digest("hex") !== value.sha256 ||
    !validSignature
  ) {
    throw new Error("Clearra job service returned a mismatched render artifact.");
  }
  return Object.freeze({ ...value });
}

function containsForbiddenAlternativeMetadata(value) {
  if (Array.isArray(value)) return value.some(containsForbiddenAlternativeMetadata);
  if (!value || typeof value !== "object") return false;
  return Object.entries(value).some(([key, nested]) => {
    const normalized = key.toLowerCase().replaceAll("-", "_");
    return FORBIDDEN_ALTERNATIVE_RESULT_KEYS.has(normalized) ||
      normalized.startsWith("portfolio_alternative_") ||
      normalized.startsWith("tie_") ||
      containsForbiddenAlternativeMetadata(nested);
  });
}

function abortableDelay(milliseconds, signal) {
  return new Promise((resolve, reject) => {
    const abort = () => {
      clearTimeout(timeout);
      reject(signal.reason ?? abortError("Clearra search was cancelled."));
    };
    const timeout = setTimeout(() => {
      signal.removeEventListener("abort", abort);
      resolve();
    }, milliseconds);
    signal.addEventListener("abort", abort, { once: true });
    if (signal.aborted) abort();
  });
}

function timeoutLabel(milliseconds) {
  if (milliseconds % 60_000 === 0) return `${milliseconds / 60_000}-minute`;
  if (milliseconds % 1_000 === 0) return `${milliseconds / 1_000}-second`;
  return `${milliseconds}-millisecond`;
}

function abortError(message) {
  const error = new Error(message);
  error.name = "AbortError";
  return error;
}
