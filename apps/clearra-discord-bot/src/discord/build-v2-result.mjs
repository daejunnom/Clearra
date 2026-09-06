const DISPLAY_FAMILY_LIMIT = 24;

const BUILD_V2_RESULT_CONTRACTS = new Map([
  ["build.cover", ["build-coverage-portfolio.v2", "portfolio"]],
  ["build.setup", ["build-target-family.v2", "candidate-family"]],
  ["build.congruent", ["build-congruence-family.v1", "candidate-family"]],
  ["build.congruent-cover", ["build-congruence-coverage.v1", "portfolio"]],
  ["build.setup-cover", ["build-setup-cover.v1", "portfolio"]],
  ["build.setup-cover-percent", ["build-setup-cover-probability.v1", "probability"]],
  ["build.setup-cover-score", ["build-setup-cover-score.v1", "score-portfolio"]],
  ["build.evaluate.cover", ["build-supplied-coverage.v1", "candidate-family"]],
  ["build.evaluate.minimals", ["build-supplied-minimum-cover.v1", "portfolio"]],
  ["build.evaluate.score", ["build-supplied-score.v1", "score-portfolio"]],
  ["build.highest-score-minimum-set", ["build-probability-score-minimum.v1", "score-portfolio"]],
  ["build.evaluate.b2b-cover", ["build-supplied-b2b-coverage.v1", "candidate-family"]],
  ["build.evaluate.cover-percent", ["build-supplied-probability.v1", "probability"]],
]);

const FORBIDDEN_ALTERNATIVE_KEY =
  /(?:^|_)(?:ties?|alternatives?|cursor|tie_metadata|alternative_metadata)(?:_|$)/u;
const FORBIDDEN_CANDIDATE_FAMILY_KEY =
  /(?:^|_)(?:candidate_ids|canonical_candidate_ids|portfolio_ids)(?:_|$)/u;
const SCORE_ONLY_EQUALITY = "score-only";

export function validDiscordBuildV2Result(structured) {
  try {
    return projectDiscordBuildV2Result(structured) !== null;
  } catch {
    return false;
  }
}

/**
 * Narrows one typed Build v2 result to Discord's bounded, canonical-only
 * projection. Unknown non-Build payloads return null; recognized but widened
 * Build payloads fail closed.
 */
export function projectDiscordBuildV2Result(structured) {
  if (!plainObject(structured) || !plainObject(structured.summary)) return null;
  const capabilityId = structured.summary.capability_id;
  const authority = BUILD_V2_RESULT_CONTRACTS.get(capabilityId);
  if (!authority) return null;

  const [resultContract, payloadKind] = authority;
  if (
    structured.kind !== resultContract ||
    structured.contract?.command?.kind !== resultContract ||
    structured.summary.result_contract !== resultContract ||
    structured.summary.payload_kind !== payloadKind
  ) {
    throw new Error(`Discord received a mismatched ${capabilityId} result contract.`);
  }
  if (containsForbiddenAlternativeMetadata(structured)) {
    throw new Error("Discord Build v2 does not expose alternative or tie paging metadata.");
  }
  if (!plainObject(structured.summary.completeness)) {
    throw new Error(`Discord received an incomplete ${capabilityId} result shape.`);
  }
  if (payloadKind === "score-portfolio") {
    assertScoreOnlyResult(structured.summary, capabilityId);
  }

  if (capabilityId === "build.highest-score-minimum-set") {
    return projectBuildScoreMinimum(structured);
  }

  const summary = stripAttackFields(clonePlain(structured.summary));
  delete summary.page_source_available;
  delete summary.page_source_identity_sha256;
  delete summary.informational_attack_basis;
  delete summary.score_informational_attack_basis;

  let truncated = false;
  if (Array.isArray(summary.candidates)) {
    truncated ||= summary.candidates.length > DISPLAY_FAMILY_LIMIT;
    summary.candidates = summary.candidates.slice(0, DISPLAY_FAMILY_LIMIT);
  }
  if (Array.isArray(summary.winners)) {
    truncated ||= summary.winners.length > DISPLAY_FAMILY_LIMIT;
    summary.winners = summary.winners
      .slice(0, DISPLAY_FAMILY_LIMIT)
      .map((winner) => withoutAttackFields(winner));
  }
  if (Array.isArray(summary.canonical_candidate_keys)) {
    truncated ||= summary.canonical_candidate_keys.length > DISPLAY_FAMILY_LIMIT;
    summary.canonical_candidate_keys = summary.canonical_candidate_keys
      .slice(0, DISPLAY_FAMILY_LIMIT);
  }
  if (truncated) summary.discord_family_display_truncated = true;

  const projected = stripAttackFields(clonePlain(structured));
  projected.summary = summary;
  return deepFreeze(projected);
}

function projectBuildScoreMinimum(structured) {
  const source = structured.summary;
  const canonicalKeys = source.canonical_candidate_keys;
  if (
    source.objective !== "max-score-cover" ||
    source.completeness.exact_minimum_proven !== true ||
    source.completeness.score_evidence_complete !== true ||
    !canonicalDecimal(source.selected_candidate_count) ||
    !canonicalDecimal(source.required_pattern_count) ||
    !Array.isArray(canonicalKeys) ||
    canonicalKeys.length === 0 ||
    BigInt(source.selected_candidate_count) !== BigInt(canonicalKeys.length) ||
    !Array.isArray(source.winners) ||
    BigInt(source.required_pattern_count) !== BigInt(source.winners.length) ||
    canonicalKeys.some((key, index, keys) =>
      !canonicalBuildCandidateKey(key) ||
      (index > 0 && keys[index - 1].localeCompare(key, "en") >= 0)
    ) ||
    source.winners.some((winner, index, winners) =>
      !plainObject(winner) ||
      !canonicalDecimal(winner.pattern_id) ||
      !canonicalKeys.includes(winner.candidate_key) ||
      (index > 0 && BigInt(winners[index - 1].pattern_id) >= BigInt(winner.pattern_id))
    )
  ) {
    throw new Error("build.highest-score-minimum-set lacks an exact canonical portfolio.");
  }
  // Build probability assigns candidate IDs in normalized ctk1 identity order;
  // the product validator proves the same lexical order. Selecting the first
  // key therefore selects the smallest canonical candidate ID without using
  // attack or exposing a tie family on Discord.
  const canonicalCandidateKey = canonicalKeys[0];
  const canonicalWinner = source.winners.find(
    (winner) => winner.candidate_key === canonicalCandidateKey,
  );
  if (!canonicalWinner) {
    throw new Error("build.highest-score-minimum-set canonical candidate lacks score evidence.");
  }
  const projected = stripAttackFields(clonePlain(structured));
  projected.summary = {
    capability_id: "build.highest-score-minimum-set",
    result_contract: "build-probability-score-minimum.v1",
    payload_kind: "canonical-build-score-minimum-candidate",
    canonical_selection: "smallest-canonical-candidate-id",
    score_equality_basis: SCORE_ONLY_EQUALITY,
    canonical_candidate_key: canonicalCandidateKey,
    canonical_winner: withoutAttackFields(canonicalWinner),
    selected_candidate_count: source.selected_candidate_count,
    required_pattern_count: source.required_pattern_count,
    complete: true,
  };
  return deepFreeze(projected);
}

export function discordBuildV2ResultAuthority() {
  return Object.freeze([...BUILD_V2_RESULT_CONTRACTS].map(
    ([capabilityId, [resultContract, payloadKind]]) => Object.freeze({
      capabilityId,
      resultContract,
      payloadKind,
    }),
  ));
}

function assertScoreOnlyResult(summary, capabilityId) {
  if (summary.score_equality_basis !== SCORE_ONLY_EQUALITY) {
    throw new Error(`${capabilityId} must use score-only equality on Discord.`);
  }
  if (!Array.isArray(summary.winners)) {
    throw new Error(`${capabilityId} must provide its ordinary score winner family.`);
  }
  for (const winner of summary.winners) {
    if (
      !plainObject(winner) ||
      typeof winner.candidate_key !== "string" ||
      winner.candidate_key.length === 0 ||
      !canonicalDecimal(winner.score)
    ) {
      throw new Error(`${capabilityId} returned an invalid score-only winner.`);
    }
  }
}

function withoutAttackFields(value) {
  if (!plainObject(value)) return value;
  return Object.fromEntries(Object.entries(value).filter(([key]) =>
    !key.toLowerCase().includes("attack")
  ));
}

function stripAttackFields(value) {
  if (Array.isArray(value)) return value.map(stripAttackFields);
  if (!plainObject(value)) return value;
  return Object.fromEntries(Object.entries(value)
    .filter(([key]) => !key.toLowerCase().includes("attack"))
    .map(([key, nested]) => [key, stripAttackFields(nested)]));
}

function containsForbiddenAlternativeMetadata(value, path = []) {
  if (Array.isArray(value)) {
    return value.some((entry, index) =>
      containsForbiddenAlternativeMetadata(entry, [...path, index])
    );
  }
  if (!plainObject(value)) return false;
  return Object.entries(value).some(([key, nested]) => {
    const normalized = key.toLowerCase();
    const knownInternalPageSource =
      path.join(".") === "summary" &&
      ["page_source_available", "page_source_identity_sha256"].includes(normalized);
    const forbidden = FORBIDDEN_ALTERNATIVE_KEY.test(normalized) ||
      FORBIDDEN_CANDIDATE_FAMILY_KEY.test(normalized) ||
      normalized === "metadata" ||
      normalized === "portfolio_alternative_page" ||
      (normalized.includes("attack") &&
        /(?:selection|ordering|equality|tiebreak|tie_break)/u.test(normalized)) ||
      (normalized.includes("page") && !knownInternalPageSource);
    return forbidden || containsForbiddenAlternativeMetadata(nested, [...path, key]);
  });
}

function canonicalDecimal(value) {
  return typeof value === "string" && /^(?:0|[1-9][0-9]*)$/u.test(value);
}

function canonicalBuildCandidateKey(value) {
  return typeof value === "string" &&
    /^ctk1\|initial=[0-9a-f]{16}\|placements=[A-Z][A-Za-z0-9@:_+\-]*:[0-9a-f]{16}(?:,[A-Z][A-Za-z0-9@:_+\-]*:[0-9a-f]{16})*$/u.test(value);
}

function clonePlain(value) {
  if (Array.isArray(value)) return value.map(clonePlain);
  if (plainObject(value)) {
    return Object.fromEntries(
      Object.entries(value).map(([key, nested]) => [key, clonePlain(nested)]),
    );
  }
  return value;
}

function deepFreeze(value) {
  if (!value || typeof value !== "object" || Object.isFrozen(value)) return value;
  for (const nested of Object.values(value)) deepFreeze(nested);
  return Object.freeze(value);
}

function plainObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}
