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
  if (structured.kind === "pc-path-family.v2") {
    return preserveCanonicalPath(structured) ?? projectPcPath(structured);
  }
  if (structured.kind === "build-path-family.v1") {
    return preserveCanonicalPath(structured, "build") ?? projectPcPath(structured, "build");
  }
  if (structured.kind === "pc-fixed-score-witness.v2") {
    return projectPcScoreFinder(structured);
  }
  if (structured.kind === "build-fixed-score-witness.v1") {
    return projectPcScoreFinder(structured, "build");
  }
  if (structured.kind === "pc-score-summary.v2") return projectPcScore(structured);
  if (structured.kind === "build-field-average-score.v1") {
    return projectPcScore(structured, "build");
  }
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

function projectPcPath(structured, semantics = "pc") {
  const build = semantics === "build";
  const capabilityId = build ? "build.complete-replay-paths" : "pc.path";
  const resultContract = build ? "build-path-family.v1" : "pc-path-family.v2";
  const witnessContract = build ? "build-path-witness.v1" : "pc-path-witness.v2";
  const summary = assertEnvelope(structured, resultContract, capabilityId);
  if (
    summary.witness_contract !== witnessContract ||
    summary.ordering !==
      "candidate-id-ascending-then-pattern-id-ascending-then-trace-key-ascending" ||
    summary.canonical_selection !== CANONICAL_SELECTION ||
    summary.complete !== true ||
    !canonicalDecimal(summary.witness_count) ||
    !canonicalDecimal(summary.materialized_pattern_count) ||
    !Array.isArray(summary.witnesses) ||
    BigInt(summary.witness_count) !== BigInt(summary.witnesses.length)
  ) throw invalid(capabilityId, "family evidence");
  const targetTerminal = build ? summary.target_terminal_board_mask : "0x0000000000000000";
  if (
    (build && !canonicalBoardMask(targetTerminal)) ||
    (!build && Object.hasOwn(summary, "target_terminal_board_mask"))
  ) throw invalid(capabilityId, "terminal contract");

  let previous = null;
  for (const witness of summary.witnesses) {
    validatePathWitness(witness, capabilityId, targetTerminal);
    const key = [BigInt(witness.candidate_id), BigInt(witness.pattern_id), witness.normalized_trace_key];
    if (previous !== null && comparePathKey(previous, key) > 0) {
      throw invalid(capabilityId, "ordering");
    }
    previous = key;
  }
  const suppliedCanonical = summary.canonical_witness;
  if (summary.witnesses.length === 0) {
    if (suppliedCanonical !== null) {
      throw invalid(capabilityId, "core-owned canonical witness");
    }
  } else {
    if (
      !plainObject(suppliedCanonical) ||
      !samePlainValue(suppliedCanonical, summary.witnesses[0])
    ) throw invalid(capabilityId, "core-owned canonical witness");
    const suppliedCandidateId = BigInt(suppliedCanonical.candidate_id);
    if (summary.witnesses.some((witness) =>
      BigInt(witness.candidate_id) < suppliedCandidateId
    )) throw invalid(capabilityId, "core-owned canonical witness");
  }
  rejectAlternativeMetadata(structured);
  const projected = clonePlain(structured);
  projected.summary = {
    capability_id: capabilityId,
    result_contract: resultContract,
    payload_kind: build ? "canonical-build-path-witness" : "canonical-pc-path-witness",
    witness_contract: witnessContract,
    ordering: summary.ordering,
    canonical_selection: summary.canonical_selection,
    problem_id: summary.problem_id,
    complete: true,
    ...(build ? { target_terminal_board_mask: targetTerminal } : {}),
    canonical_witness: suppliedCanonical === null
      ? null
      : clonePlain(suppliedCanonical),
  };
  return deepFreeze(projected);
}

/**
 * The command/executor boundary owns canonical-only projection. Its result may
 * cross the local runner and the HTTP job client before it reaches the bot, so
 * validating an already-canonical envelope must be idempotent; it must never
 * be mistaken for a widened exhaustive family and projected a second time.
 */
function preserveCanonicalPath(structured, semantics = "pc") {
  const build = semantics === "build";
  const capabilityId = build ? "build.complete-replay-paths" : "pc.path";
  const resultContract = build ? "build-path-family.v1" : "pc-path-family.v2";
  const witnessContract = build ? "build-path-witness.v1" : "pc-path-witness.v2";
  const payloadKind = build
    ? "canonical-build-path-witness"
    : "canonical-pc-path-witness";
  const summary = structured.summary;
  if (summary.payload_kind !== payloadKind) return null;
  assertEnvelope(structured, resultContract, capabilityId);
  const expectedKeys = [
    "capability_id",
    "result_contract",
    "payload_kind",
    "witness_contract",
    "ordering",
    "canonical_selection",
    "problem_id",
    "complete",
    ...(build ? ["target_terminal_board_mask"] : []),
    "canonical_witness",
  ];
  if (
    !exactKeys(summary, expectedKeys) ||
    summary.result_contract !== resultContract ||
    summary.witness_contract !== witnessContract ||
    summary.ordering !==
      "candidate-id-ascending-then-pattern-id-ascending-then-trace-key-ascending" ||
    summary.canonical_selection !== CANONICAL_SELECTION ||
    !safeText(summary.problem_id) ||
    summary.complete !== true
  ) throw invalid(capabilityId, "canonical path envelope");
  const targetTerminal = build
    ? summary.target_terminal_board_mask
    : "0x0000000000000000";
  if (
    (build && !canonicalBoardMask(targetTerminal)) ||
    (!build && Object.hasOwn(summary, "target_terminal_board_mask"))
  ) throw invalid(capabilityId, "terminal contract");
  if (summary.canonical_witness !== null) {
    validatePathWitness(summary.canonical_witness, capabilityId, targetTerminal);
  }
  rejectAlternativeMetadata(structured);
  return deepFreeze(clonePlain(structured));
}

function validatePathWitness(witness, capabilityId, targetTerminal) {
  if (
    !plainObject(witness) ||
    !canonicalPositiveDecimalU64(witness.candidate_id) ||
    !canonicalPositiveDecimalU64(witness.producer_candidate_id) ||
    !canonicalDecimal(witness.pattern_id) ||
    !safeText(witness.trace_identity) ||
    !safeText(witness.normalized_trace_key) ||
    !canonicalDecimal(witness.consumed_piece_count) ||
    !optionalSafeText(witness.terminal_hold_piece) ||
    !Array.isArray(witness.steps)
  ) throw invalid(capabilityId, "witness");
  for (const step of witness.steps) validatePcPathStep(step, capabilityId);
  if (witness.steps.at(-1)?.board_after_line_clear_mask !== targetTerminal) {
    throw invalid(capabilityId, "terminal witness");
  }
}

function validatePcPathStep(step, capabilityId) {
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
  ) throw invalid(capabilityId, "path step evidence");
}

function projectPcScoreFinder(structured, semantics = "pc") {
  const build = semantics === "build";
  const capabilityId = build ? "build.fixed-queue-maximum-score" : "pc.score-finder";
  const resultContract = build ? "build-fixed-score-witness.v1" : "pc-fixed-score-witness.v2";
  const winnerContract = build ? "build-score-pattern-winner.v1" : "pc-score-pattern-winner.v1";
  const summary = assertEnvelope(
    structured,
    resultContract,
    capabilityId,
  );
  if (
    summary.payload_kind !== "score-pattern-winner-family" ||
    summary.score_pattern_winner_contract !== winnerContract ||
    summary.score_pattern_winner_ordering !==
      "pattern-id-ascending-then-candidate-id-ascending" ||
    summary.score_pattern_winner_equality !== SCORE_ONLY_EQUALITY ||
    summary.score_pattern_canonical_selection !== CANONICAL_SELECTION ||
    summary.score_pattern_winner_complete !== true ||
    !canonicalDecimal(summary.score_pattern_winner_count) ||
    !Array.isArray(summary.score_pattern_winners) ||
    BigInt(summary.score_pattern_winner_count) !==
      BigInt(summary.score_pattern_winners.length) ||
    summary.score_pattern_winners.length === 0
  ) throw invalid(capabilityId, "winner family evidence");

  const suppliedCanonical = summary.score_pattern_canonical_winner;
  validateScorePatternWinner(suppliedCanonical, winnerContract, capabilityId);
  let previous = null;
  let suppliedWitnessMatches = 0;
  const suppliedCandidateId = BigInt(suppliedCanonical.candidate_id);
  for (const winner of summary.score_pattern_winners) {
    validateScorePatternWinner(winner, winnerContract, capabilityId);
    if (
      winner.pattern_id !== suppliedCanonical.pattern_id ||
      winner.score !== suppliedCanonical.score
    ) throw invalid(capabilityId, "score-only winner equality");
    const identity = [BigInt(winner.pattern_id), BigInt(winner.candidate_id)];
    if (previous !== null && compareBigIntPair(previous, identity) >= 0) {
      throw invalid(capabilityId, "winner ordering");
    }
    if (BigInt(winner.candidate_id) < suppliedCandidateId) {
      throw invalid(capabilityId, "core-owned canonical winner");
    }
    if (sameScorePatternWinner(winner, suppliedCanonical)) suppliedWitnessMatches += 1;
    previous = identity;
  }
  if (suppliedWitnessMatches !== 1) {
    throw invalid(capabilityId, "core-owned canonical winner");
  }
  rejectAlternativeMetadata(structured, new Set([
    "score_pattern_winner_contract",
    "score_pattern_winner_equality",
    "score_pattern_winner_ordering",
    "score_pattern_winners",
  ]));

  const projected = clonePlain(structured);
  projected.summary = {
    capability_id: capabilityId,
    result_contract: resultContract,
    payload_kind: "canonical-score-winner",
    winner_contract: winnerContract,
    ordering: "candidate-id-ascending",
    score_equality: SCORE_ONLY_SUMMARY_EQUALITY,
    canonical_selection: summary.score_pattern_canonical_selection,
    complete: true,
    canonical_winner: stripAttackFields(clonePlain(suppliedCanonical)),
  };
  return deepFreeze(stripAttackFields(projected));
}

function validateScorePatternWinner(winner, winnerContract, capabilityId) {
  if (
    !plainObject(winner) ||
    winner.contract !== winnerContract ||
    !canonicalDecimal(winner.pattern_id) ||
    !canonicalPositiveDecimalU64(winner.candidate_id) ||
    !canonicalInteger(winner.score) ||
    !canonicalDecimal(winner.informational_attack) ||
    winner.informational_attack_basis !== "canonical-equal-score-trace" ||
    !safeText(winner.normalized_solution_key)
  ) throw invalid(capabilityId, "winner");
}

function sameScorePatternWinner(left, right) {
  return left.contract === right.contract &&
    left.pattern_id === right.pattern_id &&
    left.candidate_id === right.candidate_id &&
    left.normalized_solution_key === right.normalized_solution_key &&
    left.score === right.score &&
    left.informational_attack === right.informational_attack &&
    left.informational_attack_basis === right.informational_attack_basis;
}

function compareBigIntPair(left, right) {
  if (left[0] < right[0]) return -1;
  if (left[0] > right[0]) return 1;
  if (left[1] < right[1]) return -1;
  if (left[1] > right[1]) return 1;
  return 0;
}

function projectPcScore(structured, semantics = "pc") {
  const build = semantics === "build";
  const capabilityId = build ? "build.field-average-score" : "pc.score";
  const resultContract = build ? "build-field-average-score.v1" : "pc-score-summary.v2";
  const fieldContract = build
    ? "build-solution-field-average.v1"
    : "pc-score-solution-field-average.v1";
  const summary = assertEnvelope(structured, resultContract, capabilityId);
  if (
    !exactKeys(structured, [
      "schema_version",
      "kind",
      "summary",
      "contract",
      "runtime_identity",
    ]) ||
    structured.schema_version !== 2 ||
    !validTerminalRuntimeIdentity(structured.runtime_identity) ||
    !exactKeys(summary, [
      "capability_id",
      "result_contract",
      "payload_kind",
      "score_solution_field_contract",
      "score_solution_field_ordering",
      "score_solution_field_average_basis",
      "score_evaluation_basis",
      "score_evaluation_scope",
      "score_overall_basis",
      "piece_source_id",
      "pattern_universe_id",
      "pattern_weight_model_id",
      "materialized_pattern_count",
      "score_solution_field_count",
      "score_success_pattern_count",
      "score_failed_pc_pattern_count",
      "score_covered_probability",
      "score_overall_score",
      "score_covered_pattern_conditional_average_score",
      "score_summary_complete",
      "score_solution_fields",
    ]) ||
    summary.payload_kind !== "pc-score-field-summary" ||
    summary.result_contract !== resultContract ||
    summary.score_solution_field_contract !== fieldContract ||
    summary.score_solution_field_ordering !== "normalized-solution-field-order" ||
    summary.score_solution_field_average_basis !==
      "whole-materialized-pattern-universe-failed-pc-zero" ||
    summary.score_evaluation_basis !== "all-traces" ||
    summary.score_evaluation_scope !== "full" ||
    summary.score_overall_basis !== "all-materialized-patterns-failed-pc-zero" ||
    summary.score_summary_complete !== true ||
    !canonicalPositiveDecimalU64(summary.piece_source_id) ||
    !canonicalPositiveDecimalU64(summary.pattern_universe_id) ||
    !canonicalPositiveDecimalU64(summary.pattern_weight_model_id) ||
    !canonicalDecimal(summary.materialized_pattern_count) ||
    !canonicalDecimal(summary.score_solution_field_count) ||
    !canonicalDecimal(summary.score_success_pattern_count) ||
    !canonicalDecimal(summary.score_failed_pc_pattern_count) ||
    !canonicalUnitDecimal(summary.score_covered_probability) ||
    !canonicalNonNegativeNumber(summary.score_overall_score) ||
    !optionalCanonicalNonNegativeNumber(
      summary.score_covered_pattern_conditional_average_score,
    ) ||
    !Array.isArray(summary.score_solution_fields) ||
    BigInt(summary.score_solution_field_count) !==
      BigInt(summary.score_solution_fields.length) ||
    BigInt(summary.score_success_pattern_count) +
      BigInt(summary.score_failed_pc_pattern_count) !==
      BigInt(summary.materialized_pattern_count)
  ) throw invalid(capabilityId, "all-solution field score evidence");
  let previousFieldKey = null;
  for (const field of summary.score_solution_fields) {
    if (
      !plainObject(field) ||
      !exactKeys(field, [
        "normalized_field_key",
        "average_score",
        "covered_pattern_count",
        "pattern_count",
        "score_complete",
      ]) ||
      !safeText(field.normalized_field_key) ||
      !canonicalNonNegativeNumber(field.average_score) ||
      !canonicalDecimal(field.covered_pattern_count) ||
      field.pattern_count !== summary.materialized_pattern_count ||
      field.score_complete !== true ||
      BigInt(field.covered_pattern_count) > BigInt(field.pattern_count) ||
      (previousFieldKey !== null &&
        previousFieldKey.localeCompare(field.normalized_field_key, "en") >= 0)
    ) throw invalid(capabilityId, "solution field score row");
    previousFieldKey = field.normalized_field_key;
  }
  rejectLegacyPrivateMetadata(structured);
  rejectAlternativeMetadata(structured);
  return deepFreeze(stripAttackFields(clonePlain(structured)));
}

function projectPcMinimals(structured) {
  const summary = assertEnvelope(structured, "pc-minimum-cover.v2", "pc.minimals");
  const members = canonicalPortfolioMembers(summary, "pc.minimals");
  if (members.length === 0) throw invalid("pc.minimals", "empty canonical portfolio");
  if (
    summary.canonical_selection !== CANONICAL_SELECTION ||
    !plainObject(summary.canonical_witness) ||
    !samePlainValue(summary.canonical_witness, members[0])
  ) throw invalid("pc.minimals", "core-owned canonical witness");
  const suppliedCandidateId = BigInt(summary.canonical_witness.candidate_id);
  if (members.some((member) => BigInt(member.candidate_id) < suppliedCandidateId)) {
    throw invalid("pc.minimals", "core-owned canonical witness");
  }
  const projected = clonePlain(structured);
  projected.summary = {
    capability_id: "pc.minimals",
    result_contract: "pc-minimum-cover.v2",
    payload_kind: "canonical-minimum-cover-candidate",
    canonical_selection: summary.canonical_selection,
    canonical_candidate: clonePlain(summary.canonical_witness),
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
      !canonicalPositiveDecimalU64(member.candidate_id) ||
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

function rejectLegacyPrivateMetadata(value) {
  const legacyPrivateKeys = new Set([
    "execution_authority",
    "exact_scoring_execution_batches",
    "memory_evidence",
    "pc_score_problem_evidence",
    "postprocess_score_cells",
    "problem_owner",
    "score_accuracy_level",
    "score_profile_specific_exact",
  ]);
  const visit = (nested, path = []) => {
    if (Array.isArray(nested)) {
      nested.forEach((entry, index) => visit(entry, [...path, index]));
      return;
    }
    if (!plainObject(nested)) return;
    for (const [key, child] of Object.entries(nested)) {
      const normalized = key.toLowerCase().replaceAll("-", "_");
      if (
        normalized.startsWith("private_") ||
        normalized.startsWith("transient_") ||
        legacyPrivateKeys.has(normalized)
      ) throw new Error(`${[...path, key].join(".")} is not exposed by Discord.`);
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

function canonicalPositiveDecimalU64(value) {
  return typeof value === "string" && /^[1-9][0-9]*$/u.test(value) &&
    BigInt(value) <= 18_446_744_073_709_551_615n;
}

function canonicalBoardMask(value) {
  return typeof value === "string" && /^0x[0-9a-f]{16}$/u.test(value);
}

function canonicalNonNegativeNumber(value) {
  return typeof value === "string" &&
    /^(?:0|[1-9][0-9]*)(?:\.[0-9]+)?$/u.test(value) &&
    Number.isFinite(Number(value));
}

function optionalCanonicalNonNegativeNumber(value) {
  return value === null || canonicalNonNegativeNumber(value);
}

function canonicalUnitDecimal(value) {
  return canonicalNonNegativeNumber(value) && Number(value) <= 1;
}

function validTerminalRuntimeIdentity(identity) {
  return plainObject(identity) &&
    exactKeys(identity, [
      "engine_build_id",
      "source_commit",
      "contract_schema_version",
      "supply_semantics_id",
      "artifact_schema_version",
    ]) &&
    safeText(identity.engine_build_id) &&
    identity.engine_build_id === identity.source_commit &&
    identity.contract_schema_version === "clearra.search.contract.v2" &&
    identity.supply_semantics_id ===
      "clearra.supply.projected-terminal-lookahead.v1" &&
    identity.artifact_schema_version === "clearra.solution-data.v1";
}

function exactKeys(value, expected) {
  const keys = Object.keys(value);
  return keys.length === expected.length && expected.every((key) => hasOwn(value, key));
}

function samePlainValue(left, right) {
  if (left === right) return true;
  if (Array.isArray(left) || Array.isArray(right)) {
    return Array.isArray(left) && Array.isArray(right) &&
      left.length === right.length &&
      left.every((value, index) => samePlainValue(value, right[index]));
  }
  if (!plainObject(left) || !plainObject(right)) return false;
  const leftKeys = Object.keys(left);
  const rightKeys = Object.keys(right);
  return leftKeys.length === rightKeys.length &&
    leftKeys.every((key) => hasOwn(right, key) && samePlainValue(left[key], right[key]));
}

function hasOwn(value, key) {
  return Object.prototype.hasOwnProperty.call(value, key);
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
