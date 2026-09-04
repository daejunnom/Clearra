import assert from "node:assert/strict";
import test from "node:test";

import { assertDiscordCanonicalOnlyResult } from "../src/clearra/command.mjs";
import {
  projectDiscordTypedProductResult,
  validDiscordTypedProductResult,
} from "../src/discord/typed-product-result.mjs";

test("pc.path displays the supplied numeric-smallest canonical witness with replay evidence", () => {
  const witnesses = Array.from({ length: 30 }, (_, index) => pathWitness(
    String(index + 1),
    "0",
    `trace-${String(index).padStart(2, "0")}`,
  ));
  const result = envelope("pc-path-family.v2", {
    capability_id: "pc.path",
    witness_contract: "pc-path-witness.v2",
    ordering: "candidate-id-ascending-then-pattern-id-ascending-then-trace-key-ascending",
    problem_id: "problem",
    materialized_pattern_count: "30",
    witness_count: "30",
    complete: true,
    canonical_selection: "smallest-canonical-candidate-id",
    canonical_witness: structuredClone(witnesses[0]),
    witnesses,
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

test("pc.path fails closed when the supplied canonical witness is missing or mismatched", () => {
  const first = pathWitness("2", "0", "trace-0");
  const second = pathWitness("9", "0", "trace-1");
  const result = envelope("pc-path-family.v2", {
    capability_id: "pc.path",
    witness_contract: "pc-path-witness.v2",
    ordering: "candidate-id-ascending-then-pattern-id-ascending-then-trace-key-ascending",
    problem_id: "problem",
    materialized_pattern_count: "2",
    witness_count: "2",
    complete: true,
    canonical_selection: "smallest-canonical-candidate-id",
    canonical_witness: structuredClone(first),
    witnesses: [first, second],
  });
  assert.equal(validDiscordTypedProductResult(result), true);

  const missing = structuredClone(result);
  delete missing.summary.canonical_witness;
  assert.equal(validDiscordTypedProductResult(missing), false);

  const mismatched = structuredClone(result);
  mismatched.summary.canonical_witness = structuredClone(second);
  assert.equal(validDiscordTypedProductResult(mismatched), false);

  const forged = structuredClone(result);
  forged.summary.canonical_witness.trace_identity = "forged";
  assert.equal(validDiscordTypedProductResult(forged), false);
});

test("pc.score-finder validates and displays the core-owned witness without attack or tie data", () => {
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
  reversedAttack.summary.score_pattern_canonical_winner.informational_attack = "0";
  assert.equal(
    projectDiscordTypedProductResult(reversedAttack).summary.canonical_winner.candidate_id,
    "2",
  );
});

test("pc.score-finder fails closed when the core-owned witness is absent or mismatched", () => {
  const missingSelection = scoreFinder();
  delete missingSelection.summary.score_pattern_canonical_selection;
  assert.equal(validDiscordTypedProductResult(missingSelection), false);

  const missingWitness = scoreFinder();
  delete missingWitness.summary.score_pattern_canonical_winner;
  assert.equal(validDiscordTypedProductResult(missingWitness), false);

  const mismatchedWitness = scoreFinder();
  mismatchedWitness.summary.score_pattern_canonical_winner.normalized_solution_key = "forged";
  assert.equal(validDiscordTypedProductResult(mismatchedWitness), false);

  const nonCanonicalExistingWinner = scoreFinder();
  nonCanonicalExistingWinner.summary.score_pattern_canonical_winner = structuredClone(
    nonCanonicalExistingWinner.summary.score_pattern_winners[0],
  );
  assert.equal(validDiscordTypedProductResult(nonCanonicalExistingWinner), false);

  for (const candidateId of ["0", "18446744073709551616"]) {
    const nonU64Witness = scoreFinder();
    nonU64Witness.summary.score_pattern_winners[1].candidate_id = candidateId;
    nonU64Witness.summary.score_pattern_canonical_winner.candidate_id = candidateId;
    assert.equal(validDiscordTypedProductResult(nonU64Witness), false);
  }
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

test("pc.minimals exposes only its supplied numeric-smallest canonical member", () => {
  const result = coverage("pc-minimum-cover.v2", "pc.minimals");
  const projected = projectDiscordTypedProductResult(result);
  assert.equal(projected.summary.canonical_candidate.candidate_id, "2");
  assert.equal(Object.hasOwn(projected.summary, "members"), false);
  assert.equal(JSON.stringify(projected).toLowerCase().includes("alternative"), false);
  assert.equal(JSON.stringify(projected).toLowerCase().includes("page_"), false);
});

test("pc.score preserves score-only evidence and strips informational attack", () => {
  const result = scoreSummary();
  const projected = projectDiscordTypedProductResult(result);
  assert.equal(projected.summary.score_overall_score, "600");
  assert.equal(projected.summary.score_solution_fields.length, 2);
  assert.equal(JSON.stringify(projected).toLowerCase().includes("attack"), false);
});

test("pc.score fails closed on stale, incomplete, or widened solution-field rows", () => {
  const stale = scoreSummary();
  stale.summary.payload_kind = "score-summary";
  assert.equal(validDiscordTypedProductResult(stale), false);

  const incomplete = scoreSummary();
  incomplete.summary.score_solution_fields[0].score_complete = false;
  assert.equal(validDiscordTypedProductResult(incomplete), false);

  const widened = scoreSummary();
  widened.summary.score_solution_fields[0].candidate_id = "1";
  assert.equal(validDiscordTypedProductResult(widened), false);

  const mismatchedRuntime = scoreSummary();
  mismatchedRuntime.runtime_identity.engine_build_id = "different-build";
  assert.equal(validDiscordTypedProductResult(mismatchedRuntime), false);

  const widenedEnvelope = scoreSummary();
  widenedEnvelope.resource_report = {};
  assert.equal(validDiscordTypedProductResult(widenedEnvelope), false);
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
    score_pattern_canonical_selection: "smallest-canonical-candidate-id",
    score_pattern_winner_count: "2",
    score_pattern_winner_complete: true,
    score_pattern_winners: [
      winner("0", "10", "9"),
      winner("1", "2", "1"),
    ],
    score_pattern_canonical_winner: winner("1", "2", "1"),
  });
}

function scoreSummary() {
  return {
    ...envelope("pc-score-summary.v2", {
      capability_id: "pc.score",
      result_contract: "pc-score-summary.v2",
      payload_kind: "pc-score-field-summary",
      score_solution_field_contract: "pc-score-solution-field-average.v1",
      score_solution_field_ordering: "normalized-solution-field-order",
      score_solution_field_average_basis:
        "whole-materialized-pattern-universe-failed-pc-zero",
      score_evaluation_basis: "all-traces",
      score_evaluation_scope: "full",
      score_overall_basis: "all-materialized-patterns-failed-pc-zero",
      piece_source_id: "1",
      pattern_universe_id: "2",
      pattern_weight_model_id: "3",
      materialized_pattern_count: "2",
      score_solution_field_count: "2",
      score_success_pattern_count: "1",
      score_failed_pc_pattern_count: "1",
      score_covered_probability: "0.5",
      score_overall_score: "600",
      score_covered_pattern_conditional_average_score: "1200",
      score_summary_complete: true,
      score_solution_fields: [
        scoreField("field-a", "600", "1"),
        scoreField("field-b", "0", "0"),
      ],
    }),
    schema_version: 2,
    runtime_identity: {
      engine_build_id: "unverified-local-build",
      source_commit: "unverified-local-build",
      contract_schema_version: "clearra.search.contract.v2",
      supply_semantics_id: "clearra.supply.projected-terminal-lookahead.v1",
      artifact_schema_version: "clearra.solution-data.v1",
    },
  };
}

function scoreField(normalizedFieldKey, averageScore, coveredPatternCount) {
  return {
    normalized_field_key: normalizedFieldKey,
    average_score: averageScore,
    covered_pattern_count: coveredPatternCount,
    pattern_count: "2",
    score_complete: true,
  };
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
  const members = [
    { candidate_id: "2", normalized_solution_key: "spin-a" },
    { candidate_id: "10", normalized_solution_key: "spin-z" },
  ];
  const summary = {
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
    members,
    page_handle_available: true,
  };
  if (capabilityId === "pc.minimals") {
    summary.canonical_selection = "smallest-canonical-candidate-id";
    summary.canonical_witness = structuredClone(members[0]);
  }
  return envelope(kind, summary);
}

function envelope(kind, summary) {
  return {
    kind,
    contract: { command: { kind } },
    summary,
  };
}
