import { DiscordInputError } from "./i18n.mjs";

// Discord collects this two-stage request once. The CLI remains the search
// authority; this adapter only rejects ambiguous or unbounded ingress.
const MAX_SCENARIO_CHARS = 6_000;
const MAX_PATTERN_CHARS = 2_048;
const PIECES = /^[IJLOSTZ]{2,42}$/i;
const RULES = new Set(["srs-plus", "srs", "srs-x", "jstris-180", "no-kick"]);
const SPIN_PROFILES = new Set([
  "disabled", "t-spins", "t-spins-plus", "all-mini", "all-mini-plus",
  "all-spin", "all-spin-plus",
]);
const KEYS = new Set([
  "initial_board_mask", "target_board_mask", "height", "queue",
  "stage_one_count", "placements", "role_masks", "max_early_placements",
  "borrow_role_position", "borrow_placement_mask", "hold", "rule",
  "spin_profile", "initial_b2b", "preserve_b2b_stage_one",
  "preserve_b2b_stage_two", "preserve_b2b_bags", "max_states",
  "queue_pattern", "max_pattern_evaluations", "max_total_states",
]);

function invalid(message) {
  return new DiscordInputError("options.invalid", { option: "scenario" }, message);
}

function integer(value, name, minimum, maximum) {
  if (!Number.isSafeInteger(value) || value < minimum || value > maximum) {
    throw invalid(`${name} must be an integer from ${minimum} through ${maximum}.`);
  }
  return value;
}

function boolean(value, name, fallback) {
  if (value === undefined) return fallback;
  if (typeof value !== "boolean") throw invalid(`${name} must be true or false.`);
  return value;
}

function mask(value, name, fieldLimit, exactPiece = false) {
  if (typeof value !== "string" || !/^0x[0-9a-f]{1,64}$/i.test(value)) {
    throw invalid(`${name} must be a hexadecimal board mask.`);
  }
  const parsed = BigInt(value);
  if (parsed >= fieldLimit || (exactPiece && bitCount(parsed) !== 4)) {
    throw invalid(`${name} is outside the declared field or is not four cells.`);
  }
  return `0x${parsed.toString(16)}`;
}

function bitCount(value) {
  let count = 0;
  for (let bits = value; bits > 0n; bits &= bits - 1n) count += 1;
  return count;
}

function checkedScenario(value) {
  if (typeof value !== "string" || !value.trim() || value.length > MAX_SCENARIO_CHARS) {
    throw invalid(`scenario must be nonempty JSON within ${MAX_SCENARIO_CHARS} characters.`);
  }
  let scenario;
  try {
    scenario = JSON.parse(value);
  } catch {
    throw invalid("scenario must be one JSON object.");
  }
  if (!scenario || Array.isArray(scenario) || typeof scenario !== "object" ||
      Object.keys(scenario).some((key) => !KEYS.has(key))) {
    throw invalid("scenario contains an unknown field or is not one JSON object.");
  }
  return scenario;
}

export function boundaryRecoveryArguments(command, scenarioText) {
  const scenario = checkedScenario(scenarioText);
  const height = integer(scenario.height, "height", 1, 25);
  const fieldLimit = 1n << BigInt(height * 10);
  const initial = mask(scenario.initial_board_mask, "initial_board_mask", fieldLimit);
  const target = mask(scenario.target_board_mask, "target_board_mask", fieldLimit);
  const queue = typeof scenario.queue === "string" && PIECES.test(scenario.queue)
    ? scenario.queue.toUpperCase() : null;
  if (queue === null) throw invalid("queue must contain 2 through 42 exact IOTSZJL pieces.");
  const placements = integer(scenario.placements, "placements", 2, queue.length);
  const stageOne = integer(scenario.stage_one_count, "stage_one_count", 1, placements - 1);
  const early = integer(scenario.max_early_placements ?? 1, "max_early_placements", 0, 1);
  const maxStates = integer(scenario.max_states ?? 100_000, "max_states", 1, 1_000_000);
  const roleMasks = scenario.role_masks === undefined ? [] : scenario.role_masks;
  if (!Array.isArray(roleMasks) || roleMasks.length > 0 && roleMasks.length !== placements) {
    throw invalid("role_masks must specify every required placement role or be omitted.");
  }
  const roles = roleMasks.map((value, index) => mask(value, `role_masks[${index}]`, fieldLimit, true));
  const borrowPosition = early === 1
    ? integer(scenario.borrow_role_position, "borrow_role_position", stageOne + 1, placements)
    : undefined;
  if (early === 0 && scenario.borrow_role_position !== undefined) {
    throw invalid("borrow_role_position requires max_early_placements=1.");
  }
  const borrowedMask = early === 1 && roles.length === 0
    ? mask(scenario.borrow_placement_mask, "borrow_placement_mask", fieldLimit, true)
    : undefined;
  if (early === 0 && scenario.borrow_placement_mask !== undefined) {
    throw invalid("borrow_placement_mask requires max_early_placements=1.");
  }
  if (scenario.borrow_placement_mask !== undefined && roles.length > 0 &&
      mask(scenario.borrow_placement_mask, "borrow_placement_mask", fieldLimit, true) !== roles[borrowPosition - 1]) {
    throw invalid("borrow_placement_mask must match the selected placement role.");
  }
  const hold = boolean(scenario.hold, "hold", true);
  const initialB2B = boolean(scenario.initial_b2b, "initial_b2b", true);
  const stageOneB2B = boolean(scenario.preserve_b2b_stage_one, "preserve_b2b_stage_one", false);
  const stageTwoB2B = boolean(scenario.preserve_b2b_stage_two, "preserve_b2b_stage_two", false);
  const rule = scenario.rule ?? "srs-plus";
  const spinProfile = scenario.spin_profile ?? "all-spin-plus";
  if (!RULES.has(rule) || !SPIN_PROFILES.has(spinProfile)) {
    throw invalid("rule or spin_profile is not supported by boundary recovery.");
  }
  const bagCount = Math.ceil(stageOne / 7) + Math.ceil((placements - stageOne) / 7);
  const b2bBags = scenario.preserve_b2b_bags ?? [];
  if (!Array.isArray(b2bBags) || new Set(b2bBags).size !== b2bBags.length) {
    throw invalid("preserve_b2b_bags must be a list of distinct bag positions.");
  }
  for (const bag of b2bBags) integer(bag, "preserve_b2b_bags entry", 1, bagCount);
  const pattern = scenario.queue_pattern;
  if (pattern !== undefined &&
      (typeof pattern !== "string" || !pattern.trim() || pattern.length > MAX_PATTERN_CHARS ||
       queue.length % 7 !== 0 || stageOne % 7 !== 0 || placements !== queue.length ||
       roles.length !== placements ||
       Array.from({ length: queue.length / 7 }, (_, index) =>
         new Set(queue.slice(index * 7, index * 7 + 7)).size !== 7).some(Boolean))) {
    throw invalid("queue_pattern requires complete seven-piece reference bags and exact roles.");
  }
  if (pattern === undefined &&
      (scenario.max_pattern_evaluations !== undefined || scenario.max_total_states !== undefined)) {
    throw invalid("pattern budgets require queue_pattern.");
  }
  const args = [
    ...command.argvPrefix,
    "--initial-board-mask", initial,
    "--target-board-mask", target,
    "--height", String(height),
    "--queue", queue,
    "--stage-one-count", String(stageOne),
    "--placements", String(placements),
    "--max-early-placements", String(early),
    ...(borrowPosition === undefined ? [] : ["--borrow-role-position", String(borrowPosition)]),
    ...(borrowedMask === undefined ? [] : ["--borrow-placement-mask", borrowedMask]),
    hold ? "--hold" : "--no-hold",
    "--rule", rule,
    "--spin-profile", spinProfile,
    "--initial-b2b", initialB2B ? "1" : "0",
    "--max-states", String(maxStates),
  ];
  roles.forEach((value, index) => args.push("--role-mask", `${index + 1}:${value}`));
  if (stageOneB2B) args.push("--preserve-b2b-stage-one");
  if (stageTwoB2B) args.push("--preserve-b2b-stage-two");
  b2bBags.forEach((bag) => args.push("--preserve-b2b-bag", String(bag)));
  if (pattern !== undefined) {
    args.push("--queue-pattern", pattern.trim());
    args.push("--max-pattern-evaluations", String(integer(
      scenario.max_pattern_evaluations ?? 100, "max_pattern_evaluations", 1, 100_000,
    )));
    args.push("--max-total-states", String(integer(
      scenario.max_total_states ?? 1_000_000, "max_total_states", 1, 100_000_000,
    )));
  }
  return args;
}
