import assert from "node:assert/strict";
import test from "node:test";

import { assertDiscordCanonicalOnlyResult } from "../src/clearra/command.mjs";
import {
  projectDiscordTypedProductResult,
  validDiscordTypedProductResult,
} from "../src/discord/typed-product-result.mjs";

test("pc.path selects one numeric-smallest canonical witness with its replay evidence", () => {
  const result = envelope("pc-path-family.v2", {
    capability_id: "pc.path",
    witness_contract: "pc-path-witness.v2",
    ordering: "candidate-id-ascending-then-pattern-id-ascending-then-trace-key-ascending",
    problem_id: "problem",
    materialized_pattern_count: "30",
    witness_count: "30",
    complete: true,
    witnesses: Array.from({ length: 30 }, (_, index) => pathWitness(
      String(index + 1),
      "0",
      `trace-${String(index).padStart(2, "0")}`,
    )),
  });
  const projected = projectDiscordTypedProductResult(result);
  assert.equal(projected.summary.canonical_selection, "smallest-canonical-candidate-id");
  assert.equal(projected.summary.canonical_witness.candidate_id, "1");
  assert.equal(projected.summary.canonical_witness.steps[0].input_cursor, "0");
  assert.equal(Object.hasOwn(projected.summary, "witness_count"), false);
  assert.equal(Object.hasOwn(projected.summary, "witnesses"), false);
  assert.equal(JSON.stringify(projected).toLowerCase().includes("tie"), false);
  assert.equal(validDiscordTypedProductResult(result), true);
});

test("pc.score-finder selects numeric smallest candidate ID without attack or tie data", () => {
  const result = scoreFinder();
  const projected = projectDiscordTypedProductResult(result);
  assert.equal(projected.summary.canonical_selection, "smallest-canonical-candidate-id");
  assert.equal(projected.summary.canonical_winner.candidate_id, "2");
  assert.equal(projected.summary.canonical_winner.score, "1200");
  assert.equal(projected.summary.score_equality, "score-only");
  assert.equal(JSON.stringify(projected).toLowerCase().includes("attack"), false);
  assert.equal(JSON.stringify(projected).toLowerCase().includes("winner_count"), false);
  assert.equal(JSON.stringify(projected).toLowerCase().includes("tie"), false);

  const reversedAttack = structuredClone(result);
  reversedAttack.summary.score_pattern_winners[0].informational_attack = "999";
  reversedAttack.summary.score_pattern_winners[1].informational_attack = "0";
  assert.equal(
    projectDiscordTypedProductResult(reversedAttack).summary.canonical_winner.candidate_id,
    "2",
  );
});

test("setup and spin ordinary product families remain bounded ordinary families", () => {
  const setup = envelope("setup-joint-ranking.v2", {
    capability_id: "setup.joint",
    result_contract: "setup-joint-ranking.v2",
    payload_kind: "setup-ranked-family",
    ordering: "rank-ascending",
    candidate_count: "30",
    candidates: Array.from({ length: 30 }, (_, index) => ({
      candidate_id: `candidate-${index}`,
      condition_id: `condition-${index}`,
      setup_id: `setup-${index}`,
    })),
  });
  const setupProjected = projectDiscordTypedProductResult(setup);
  assert.equal(setupProjected.summary.candidates.length, 24);
  assert.equal(setupProjected.summary.candidate_count, "30");

  const spin = spinFamily("spin-structure-family.v2", "spin-structure.search", 30);
  const spinProjected = projectDiscordTypedProductResult(spin);
  assert.equal(spinProjected.summary.candidates.length, 24);
  assert.equal(spinProjected.summary.complete, true);
});

test("setup.score and guaranteed spin validate completeness before bounding", () => {
  const setupScore = envelope("setup-score-ranking.v1", {
    capability_id: "setup.score",
    result_contract: "setup-score-ranking.v1",
    payload_kind: "ranked-family",
    ordering: "priority-descending",
    candidate_count: "1",
    complete: true,
    candidates: [{ rank: "1", candidate_id: "setup-score:1" }],
  });
  assert.equal(projectDiscordTypedProductResult(setupScore).summary.candidates.length, 1);

  const guaranteed = spinFamily(
    "spin-structure-guaranteed.v1",
    "spin-structure.guaranteed",
    1,
  );
  guaranteed.summary.guaranteed_final_piece = "T";
  guaranteed.summary.guarantee_basis = "final-piece-required";
  guaranteed.summary.dependency_report_included = false;
  assert.equal(validDiscordTypedProductResult(guaranteed), true);

  const incomplete = structuredClone(guaranteed);
  incomplete.summary.complete = false;
  assert.equal(validDiscordTypedProductResult(incomplete), false);
});

test("spin cover preserves canonical portfolio membership without alternative paging metadata", () => {
  const result = coverage("spin-structure-coverage.v1", "spin-structure.cover");
  const projected = projectDiscordTypedProductResult(result);
  assert.equal(projected.summary.canonical_selection, "first-canonical-portfolio");
  assert.deepEqual(
    projected.summary.displayed_members.map(({ candidate_id }) => candidate_id),
    ["2", "10"],
  );
  for (const forbidden of [
    "alternative_index",
    "known_alternative_count",
    "total_alternative_count",
    "member_page_number",
    "total_member_pages",
    "page_handle_available",
  ]) assert.equal(Object.hasOwn(projected.summary, forbidden), false, forbidden);
});

test("pc.minimals exposes only its numeric smallest canonical member", () => {
  const result = coverage("pc-minimum-cover.v2", "pc.minimals");
  const projected = projectDiscordTypedProductResult(result);
  assert.equal(projected.summary.canonical_candidate.candidate_id, "2");
  assert.equal(Object.hasOwn(projected.summary, "members"), false);
  assert.equal(JSON.stringify(projected).toLowerCase().includes("alternative"), false);
  assert.equal(JSON.stringify(projected).toLowerCase().includes("page_"), false);
});

test("pc.score preserves score-only evidence and strips informational attack", () => {
  const result = envelope("pc-score-summary.v2", {
    capability_id: "pc.score",
    score_equality_basis: "score-only",
    score_best_score: "1200",
    score_best_attack: "9",
    informational_attack_basis: "canonical-equal-score-trace",
    score_summary_complete: true,
    objective_complete: true,
    probability_complete: true,
  });
  const projected = projectDiscordTypedProductResult(result);
  assert.equal(projected.summary.score_best_score, "1200");
  assert.equal(JSON.stringify(projected).toLowerCase().includes("attack"), false);
});

test("integrated canonical-only projection rejects widened metadata and rewrites stdout", () => {
  const projected = assertDiscordCanonicalOnlyResult({
    exitCode: 0,
    stdout: JSON.stringify(scoreFinder()),
    stderr: "",
  });
  assert.equal(JSON.parse(projected.stdout).summary.canonical_winner.candidate_id, "2");

  const widened = spinFamily("spin-structure-family.v2", "spin-structure.search", 1);
  widened.summary.tie_cursor = "forbidden";
  assert.throws(
    () => projectDiscordTypedProductResult(widened),
    /not exposed by Discord/u,
  );
});

function scoreFinder() {
  return envelope("pc-fixed-score-witness.v2", {
    capability_id: "pc.score-finder",
    result_contract: "pc-fixed-score-witness.v2",
    payload_kind: "score-pattern-winner-family",
    score_pattern_winner_contract: "pc-score-pattern-winner.v1",
    score_pattern_winner_ordering: "pattern-id-ascending-then-candidate-id-ascending",
    score_pattern_winner_equality: "score-only-attack-informational",
    score_informational_attack_basis: "canonical-equal-score-trace",
    score_pattern_winner_count: "2",
    score_pattern_winner_complete: true,
    score_pattern_winners: [
      winner("0", "10", "9"),
      winner("1", "2", "1"),
    ],
  });
}

function winner(patternId, candidateId, attack) {
  return {
    contract: "pc-score-pattern-winner.v1",
    pattern_id: patternId,
    candidate_id: candidateId,
    normalized_solution_key: `solution-${candidateId}`,
    score: "1200",
    informational_attack: attack,
    informational_attack_basis: "canonical-equal-score-trace",
  };
}

function pathWitness(candidateId, patternId, traceKey) {
  return {
    candidate_id: candidateId,
    producer_candidate_id: candidateId,
    pattern_id: patternId,
    trace_identity: `identity-${traceKey}`,
    normalized_trace_key: traceKey,
    consumed_piece_count: "1",
    terminal_hold_piece: null,
    steps: [{
      step_index: "0",
      operation_id: `operation-${candidateId}`,
      active_piece: "T",
      input_cursor: "0",
      output_cursor: "1",
      input_hold_piece: null,
      output_hold_piece: null,
      hold_decision: "direct",
      rotation: "spawn",
      x: "0",
      y: "0",
      placement_mask: "000000000000000f",
      board_before_mask: "0000000000000000",
      board_after_placement_mask: "000000000000000f",
      board_after_line_clear_mask: "0000000000000000",
      cleared_row_mask: "0000000000000001",
      cleared_lines: "1",
      line_clear_identity: "rows:0000000000000001:count:1",
    }],
  };
}

function spinFamily(kind, capabilityId, count) {
  return envelope(kind, {
    capability_id: capabilityId,
    result_contract: kind,
    payload_kind: "spin-structure-family",
    ordering: "candidate-id-ascending",
    regular_count: String(count),
    mini_count: "0",
    candidate_count: String(count),
    complete: true,
    candidates: Array.from({ length: count }, (_, index) => ({
      candidate_id: String(index + 1),
      partition: "regular",
      placement_count: "1",
    })),
  });
}

function coverage(kind, capabilityId) {
  return envelope(kind, {
    capability_id: capabilityId,
    result_contract: kind,
    payload_kind: "coverage-portfolio",
    set_contract: "portfolio-alternative-set.v1",
    page_contract: "portfolio-alternative-page.v1",
    member_page_contract: "portfolio-member-page.v1",
    set_identity_sha256: "a".repeat(64),
    candidate_map_sha256: "b".repeat(64),
    alternative_index: "1",
    optimal_cardinality: "2",
    known_alternative_count: "3",
    total_alternative_count: "3",
    enumeration_complete: true,
    member_page_number: "1",
    total_member_pages: "1",
    members: [
      { candidate_id: "10", normalized_solution_key: "spin-z" },
      { candidate_id: "2", normalized_solution_key: "spin-a" },
    ],
    page_handle_available: true,
  });
}

function envelope(kind, summary) {
  return {
    kind,
    contract: { command: { kind } },
    summary,
  };
}
