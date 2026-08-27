const DISPLAY_FAMILY_LIMIT = 24;
const SCORE_ONLY_EQUALITY = "score-only-attack-informational";
const SCORE_ONLY_SUMMARY_EQUALITY = "score-only";
const CANONICAL_SELECTION = "smallest-canonical-candidate-id";

const SETUP_FAMILIES = new Map([
  ["setup-joint-ranking.v2", "setup.joint"],
  ["setup-build-ranking.v2", "setup.build"],
  ["setup-pc-ranking.v2", "setup.pc"],
]);
const SPIN_FAMILIES = new Map([
  ["spin-structure-family.v2", "spin-structure.search"],
  ["spin-structure-guaranteed.v1", "spin-structure.guaranteed"],
]);

/**
 * Projects closed App/CLI product payloads onto Discord's bounded result
 * surface. Unknown payloads return null; recognized but widened or incomplete
 * payloads fail closed.
 */
export function projectDiscordTypedProductResult(structured) {
  if (!plainObject(structured) || !plainObject(structured.summary)) return null;
  if (structured.kind === "pc-path-family.v2") return projectPcPath(structured);
  if (structured.kind === "pc-fixed-score-witness.v2") {
    return projectPcScoreFinder(structured);
  }
  if (structured.kind === "pc-score-summary.v2") return projectPcScore(structured);
  if (structured.kind === "pc-minimum-cover.v2") return projectPcMinimals(structured);
  if (structured.kind === "setup-score-ranking.v1") return projectSetupScore(structured);
  if (SETUP_FAMILIES.has(structured.kind)) return projectSetupFamily(structured);
  if (SPIN_FAMILIES.has(structured.kind)) return projectSpinFamily(structured);
  if (structured.kind === "spin-structure-coverage.v1") {
    return projectSpinCoverage(structured);
  }
  return null;
}

export function validDiscordTypedProductResult(structured) {
  try {
    return projectDiscordTypedProductResult(structured) !== null;
  } catch {
    return false;
  }
}

function projectPcPath(structured) {
  const summary = assertEnvelope(structured, "pc-path-family.v2", "pc.path");
  if (
    summary.witness_contract !== "pc-path-witness.v2" ||
    summary.ordering !==
      "candidate-id-ascending-then-pattern-id-ascending-then-trace-key-ascending" ||
    summary.complete !== true ||
    !canonicalDecimal(summary.witness_count) ||
    !canonicalDecimal(summary.materialized_pattern_count) ||
    !Array.isArray(summary.witnesses) ||
    BigInt(summary.witness_count) !== BigInt(summary.witnesses.length)
  ) throw invalid("pc.path", "family evidence");

  let previous = null;
  let canonical = null;
  for (const witness of summary.witnesses) {
    if (
      !plainObject(witness) ||
      !canonicalDecimal(witness.candidate_id) ||
      !canonicalDecimal(witness.producer_candidate_id) ||
      !canonicalDecimal(witness.pattern_id) ||
      !safeText(witness.trace_identity) ||
      !safeText(witness.normalized_trace_key) ||
      !canonicalDecimal(witness.consumed_piece_count) ||
      !optionalSafeText(witness.terminal_hold_piece) ||
      !Array.isArray(witness.steps)
    ) throw invalid("pc.path", "witness");
    for (const step of witness.steps) validatePcPathStep(step);
    const key = [BigInt(witness.candidate_id), BigInt(witness.pattern_id), witness.normalized_trace_key];
    if (previous !== null && comparePathKey(previous, key) > 0) {
      throw invalid("pc.path", "ordering");
    }
    if (canonical === null || comparePathKey(key, canonical.key) < 0) {
      canonical = { key, witness };
    }
    previous = key;
  }
  rejectAlternativeMetadata(structured);
  const projected = clonePlain(structured);
  projected.summary = {
    capability_id: "pc.path",
    result_contract: "pc-path-family.v2",
    payload_kind: "canonical-pc-path-witness",
    witness_contract: "pc-path-witness.v2",
    ordering: summary.ordering,
    canonical_selection: CANONICAL_SELECTION,
    problem_id: summary.problem_id,
    complete: true,
    canonical_witness: canonical === null ? null : clonePlain(canonical.witness),
  };
  return deepFreeze(projected);
}

function validatePcPathStep(step) {
  if (
    !plainObject(step) ||
    !canonicalDecimal(step.step_index) ||
    !safeText(step.operation_id) ||
    !safeText(step.active_piece) ||
    !canonicalDecimal(step.input_cursor) ||
    !canonicalDecimal(step.output_cursor) ||
    !optionalSafeText(step.input_hold_piece) ||
    !optionalSafeText(step.output_hold_piece) ||
    !safeText(step.hold_decision) ||
    !safeText(step.rotation) ||
    !canonicalInteger(step.x) ||
    !canonicalInteger(step.y) ||
    !safeText(step.placement_mask) ||
    !safeText(step.board_before_mask) ||
    !safeText(step.board_after_placement_mask) ||
    !safeText(step.board_after_line_clear_mask) ||
    !safeText(step.cleared_row_mask) ||
    !canonicalDecimal(step.cleared_lines) ||
    !safeText(step.line_clear_identity)
  ) throw invalid("pc.path", "path step evidence");
}

function projectPcScoreFinder(structured) {
  const summary = assertEnvelope(
    structured,
    "pc-fixed-score-witness.v2",
    "pc.score-finder",
  );
  if (
    summary.payload_kind !== "score-pattern-winner-family" ||
    summary.score_pattern_winner_contract !== "pc-score-pattern-winner.v1" ||
    summary.score_pattern_winner_ordering !==
      "pattern-id-ascending-then-candidate-id-ascending" ||
    summary.score_pattern_winner_equality !== SCORE_ONLY_EQUALITY ||
    summary.score_pattern_winner_complete !== true ||
    !canonicalDecimal(summary.score_pattern_winner_count) ||
    !Array.isArray(summary.score_pattern_winners) ||
    BigInt(summary.score_pattern_winner_count) !==
      BigInt(summary.score_pattern_winners.length) ||
    summary.score_pattern_winners.length === 0
  ) throw invalid("pc.score-finder", "winner family evidence");

  let canonical = null;
  for (const winner of summary.score_pattern_winners) {
    if (
      !plainObject(winner) ||
      winner.contract !== "pc-score-pattern-winner.v1" ||
      !canonicalDecimal(winner.pattern_id) ||
      !canonicalDecimal(winner.candidate_id) ||
      !canonicalInteger(winner.score) ||
      !safeText(winner.normalized_solution_key)
    ) throw invalid("pc.score-finder", "winner");
    if (
      canonical === null ||
      BigInt(winner.candidate_id) < BigInt(canonical.candidate_id)
    ) canonical = winner;
  }
  rejectAlternativeMetadata(structured, new Set([
    "score_pattern_winner_contract",
    "score_pattern_winner_equality",
    "score_pattern_winner_ordering",
    "score_pattern_winners",
  ]));

  const projected = clonePlain(structured);
  projected.summary = {
    capability_id: "pc.score-finder",
    result_contract: "pc-fixed-score-witness.v2",
    payload_kind: "canonical-score-winner",
    winner_contract: "pc-score-pattern-winner.v1",
    ordering: "candidate-id-ascending",
    score_equality: SCORE_ONLY_SUMMARY_EQUALITY,
    canonical_selection: CANONICAL_SELECTION,
    complete: true,
    canonical_winner: stripAttackFields(clonePlain(canonical)),
  };
  return deepFreeze(stripAttackFields(projected));
}

function projectPcScore(structured) {
  const summary = assertEnvelope(structured, "pc-score-summary.v2", "pc.score");
  if (
    summary.score_equality_basis !== SCORE_ONLY_SUMMARY_EQUALITY ||
    summary.score_summary_complete !== true ||
    summary.objective_complete !== true ||
    summary.probability_complete !== true ||
    !canonicalInteger(summary.score_best_score)
  ) throw invalid("pc.score", "score-only completeness");
  rejectAlternativeMetadata(structured);
  return deepFreeze(stripAttackFields(clonePlain(structured)));
}

function projectPcMinimals(structured) {
  const summary = assertEnvelope(structured, "pc-minimum-cover.v2", "pc.minimals");
  const members = canonicalPortfolioMembers(summary, "pc.minimals");
  if (members.length === 0) throw invalid("pc.minimals", "empty canonical portfolio");
  const canonical = members.reduce((left, right) =>
    BigInt(right.candidate_id) < BigInt(left.candidate_id) ? right : left
  );
  const projected = clonePlain(structured);
  projected.summary = {
    capability_id: "pc.minimals",
    result_contract: "pc-minimum-cover.v2",
    payload_kind: "canonical-minimum-cover-candidate",
    canonical_selection: CANONICAL_SELECTION,
    canonical_candidate: clonePlain(canonical),
  };
  return deepFreeze(projected);
}

function projectSetupFamily(structured) {
  const capabilityId = SETUP_FAMILIES.get(structured.kind);
  const summary = assertEnvelope(structured, structured.kind, capabilityId);
  if (
    summary.result_contract !== structured.kind ||
    summary.payload_kind !== "setup-ranked-family" ||
    !canonicalDecimal(summary.candidate_count) ||
    !Array.isArray(summary.candidates) ||
    BigInt(summary.candidate_count) !== BigInt(summary.candidates.length) ||
    !safeText(summary.ordering)
  ) throw invalid(capabilityId, "ranked family evidence");
  for (const candidate of summary.candidates) {
    if (
      !plainObject(candidate) ||
      !safeText(candidate.candidate_id) ||
      !safeText(candidate.condition_id) ||
      !safeText(candidate.setup_id)
    ) throw invalid(capabilityId, "ranked candidate");
  }
  rejectAlternativeMetadata(structured);
  return boundedFamily(structured, "candidates");
}

function projectSetupScore(structured) {
  const summary = assertEnvelope(structured, "setup-score-ranking.v1", "setup.score");
  if (
    summary.result_contract !== "setup-score-ranking.v1" ||
    summary.payload_kind !== "ranked-family" ||
    summary.complete !== true ||
    !canonicalDecimal(summary.candidate_count) ||
    !Array.isArray(summary.candidates) ||
    BigInt(summary.candidate_count) !== BigInt(summary.candidates.length) ||
    !safeText(summary.ordering)
  ) throw invalid("setup.score", "ranked family evidence");
  for (const candidate of summary.candidates) {
    if (
      !plainObject(candidate) ||
      !canonicalDecimal(candidate.rank) ||
      !safeText(candidate.candidate_id)
    ) throw invalid("setup.score", "ranked candidate");
  }
  rejectAlternativeMetadata(structured);
  return boundedFamily(structured, "candidates");
}

function projectSpinFamily(structured) {
  const capabilityId = SPIN_FAMILIES.get(structured.kind);
  const summary = assertEnvelope(structured, structured.kind, capabilityId);
  if (
    summary.result_contract !== structured.kind ||
    summary.payload_kind !== "spin-structure-family" ||
    summary.complete !== true ||
    !canonicalDecimal(summary.candidate_count) ||
    !canonicalDecimal(summary.regular_count) ||
    !canonicalDecimal(summary.mini_count) ||
    !Array.isArray(summary.candidates) ||
    BigInt(summary.candidate_count) !== BigInt(summary.candidates.length) ||
    !safeText(summary.ordering)
  ) throw invalid(capabilityId, "structure family evidence");
  for (const candidate of summary.candidates) {
    if (
      !plainObject(candidate) ||
      !safeText(candidate.candidate_id) ||
      !new Set(["regular", "mini"]).has(candidate.partition) ||
      !canonicalDecimal(candidate.placement_count)
    ) throw invalid(capabilityId, "structure candidate");
  }
  rejectAlternativeMetadata(structured);
  return boundedFamily(structured, "candidates");
}

function projectSpinCoverage(structured) {
  const summary = assertEnvelope(
    structured,
    "spin-structure-coverage.v1",
    "spin-structure.cover",
  );
  const members = canonicalPortfolioMembers(summary, "spin-structure.cover");
  if (
    summary.payload_kind !== "coverage-portfolio" ||
    summary.set_contract !== "portfolio-alternative-set.v1" ||
    summary.page_contract !== "portfolio-alternative-page.v1" ||
    summary.member_page_contract !== "portfolio-member-page.v1" ||
    !sha256(summary.set_identity_sha256) ||
    !sha256(summary.candidate_map_sha256) ||
    !canonicalDecimal(summary.optimal_cardinality)
  ) throw invalid("spin-structure.cover", "canonical portfolio evidence");
  const displayed = members
    .slice()
    .sort((left, right) => left.normalized_solution_key.localeCompare(
      right.normalized_solution_key,
    ))
    .slice(0, DISPLAY_FAMILY_LIMIT)
    .map(clonePlain);
  const truncated = BigInt(summary.optimal_cardinality) > BigInt(displayed.length) ||
    summary.total_member_pages !== "1";
  const projected = clonePlain(structured);
  projected.summary = {
    capability_id: "spin-structure.cover",
    result_contract: "spin-structure-coverage.v1",
    payload_kind: "canonical-coverage-portfolio",
    set_contract: "portfolio-alternative-set.v1",
    set_identity_sha256: summary.set_identity_sha256,
    candidate_map_sha256: summary.candidate_map_sha256,
    optimal_cardinality: summary.optimal_cardinality,
    canonical_selection: "first-canonical-portfolio",
    displayed_members: displayed,
    ...(truncated ? { discord_family_display_truncated: true } : {}),
  };
  return deepFreeze(projected);
}

function canonicalPortfolioMembers(summary, capabilityId) {
  if (
    summary.result_contract === undefined ||
    summary.payload_kind !== "coverage-portfolio" ||
    summary.alternative_index !== "1" ||
    summary.member_page_number !== "1" ||
    !canonicalDecimal(summary.known_alternative_count) ||
    !canonicalDecimal(summary.optimal_cardinality) ||
    !canonicalDecimal(summary.total_member_pages) ||
    !Array.isArray(summary.members)
  ) throw invalid(capabilityId, "canonical first portfolio");
  for (const member of summary.members) {
    if (
      !plainObject(member) ||
      !canonicalDecimal(member.candidate_id) ||
      !safeText(member.normalized_solution_key)
    ) throw invalid(capabilityId, "portfolio member");
  }
  return summary.members;
}

function boundedFamily(structured, field) {
  const projected = stripAttackFields(clonePlain(structured));
  const members = projected.summary[field];
  if (
    field === "candidates" &&
    canonicalDecimal(projected.summary.candidate_count) &&
    projected.summary.result_count === undefined
  ) {
    projected.summary.result_count = projected.summary.candidate_count;
  }
  if (members.length > DISPLAY_FAMILY_LIMIT) {
    projected.summary[field] = members.slice(0, DISPLAY_FAMILY_LIMIT);
    projected.summary.discord_family_display_truncated = true;
  }
  return deepFreeze(projected);
}

function assertEnvelope(structured, kind, capabilityId) {
  if (
    structured.kind !== kind ||
    structured.contract?.command?.kind !== kind ||
    structured.summary.capability_id !== capabilityId
  ) throw invalid(capabilityId, "result contract");
  return structured.summary;
}

function rejectAlternativeMetadata(value, allowed = new Set()) {
  const visit = (nested, path = []) => {
    if (Array.isArray(nested)) {
      nested.forEach((entry, index) => visit(entry, [...path, index]));
      return;
    }
    if (!plainObject(nested)) return;
    for (const [key, child] of Object.entries(nested)) {
      const normalized = key.toLowerCase().replaceAll("-", "_");
      const pathText = [...path, key].join(".");
      const traceCursor = /(?:^|\.)(?:input_cursor|output_cursor)$/u.test(pathText);
      const forbidden = !allowed.has(normalized) && (
        /(?:^|_)(?:ties?|alternatives?|tie_metadata)(?:_|$)/u.test(normalized) ||
        normalized === "cursor" ||
        (/(?:^|_)page(?:_|$)/u.test(normalized) && !traceCursor) ||
        (normalized.includes("attack") &&
          /(?:selection|ordering|equality|tiebreak|tie_break)/u.test(normalized))
      );
      if (forbidden) throw new Error(`${pathText} is not exposed by Discord.`);
      visit(child, [...path, key]);
    }
  };
  visit(value);
}

function stripAttackFields(value) {
  if (Array.isArray(value)) return value.map(stripAttackFields);
  if (!plainObject(value)) return value;
  return Object.fromEntries(Object.entries(value)
    .filter(([key]) => !key.toLowerCase().includes("attack"))
    .map(([key, nested]) => [key, stripAttackFields(nested)]));
}

function comparePathKey(left, right) {
  if (left[0] !== right[0]) return left[0] < right[0] ? -1 : 1;
  if (left[1] !== right[1]) return left[1] < right[1] ? -1 : 1;
  return left[2].localeCompare(right[2]);
}

function canonicalDecimal(value) {
  return typeof value === "string" && /^(?:0|[1-9][0-9]*)$/u.test(value);
}

function canonicalInteger(value) {
  return typeof value === "string" && /^(?:0|-?[1-9][0-9]*)$/u.test(value);
}

function safeText(value) {
  return typeof value === "string" &&
    value.length > 0 &&
    value.length <= 16_384 &&
    value.trim() === value &&
    !/[\u0000-\u001f\u007f]/u.test(value) &&
    !/@(?:everyone|here)/iu.test(value);
}

function optionalSafeText(value) {
  return value === null || safeText(value);
}

function sha256(value) {
  return typeof value === "string" && /^[0-9a-f]{64}$/u.test(value);
}

function invalid(capabilityId, subject) {
  return new Error(`Discord received invalid ${subject} for ${capabilityId}.`);
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
