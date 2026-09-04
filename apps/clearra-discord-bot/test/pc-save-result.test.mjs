import assert from "node:assert/strict";
import test from "node:test";

import {
  selectDiscordBestSaveWinner,
  validDiscordPcSaveResult,
} from "../src/discord/pc-save-result.mjs";

function group(candidateId, unconditional, conditional) {
  const encodedCandidateId = String(candidateId);
  return {
    identity: `hold:-|bag:${encodedCandidateId}`,
    successful_pattern_count: 1,
    unconditional_probability: unconditional,
    conditional_probability_given_pc: conditional,
    canonical_candidate_id: encodedCandidateId,
    witnesses: [{ candidate_id: encodedCandidateId }],
  };
}

test("Discord validates separate unconditional and conditional save probabilities", () => {
  const structured = {
    kind: "pc-save-groups.v2",
    summary: {
      save_contract: "pc-save-groups.v2",
      save_pc_probability: 0.5,
      save_groups: [group(7, 0.25, 0.5)],
    },
  };
  assert.equal(validDiscordPcSaveResult(structured, "saves"), true);
  const forged = structuredClone(structured);
  delete forged.summary.save_groups[0].conditional_probability_given_pc;
  assert.equal(validDiscordPcSaveResult(forged, "saves"), false);

  const legitimateSourceCursor = structuredClone(structured);
  legitimateSourceCursor.summary.save_groups[0].witnesses[0].source_cursor = 3;
  assert.equal(validDiscordPcSaveResult(legitimateSourceCursor, "saves"), true);
});

test("Discord displays the core-supplied smallest canonical best-save candidate", () => {
  const winners = [
    {
      weighted_total: 6,
      balanced_jl_count: 0,
      exact_group_probability: 0.25,
      group: group("9007199254740993", 0.25, 1 / 3),
    },
    {
      weighted_total: 6,
      balanced_jl_count: 0,
      exact_group_probability: 0.25,
      group: group("18446744073709551615", 0.25, 1 / 3),
    },
  ];
  const summary = {
    best_save_contract: "pc-best-save.v2",
    best_save_schema: "clearra-save-v1",
    best_save_probability_basis: "whole-universe-unconditional",
    best_save_pc_probability: 0.75,
    best_save_canonical_selection: "smallest-canonical-candidate-id",
    best_save_canonical_winner: structuredClone(winners[0]),
    best_save_winners: winners,
  };
  const structured = { kind: "pc-best-save.v2", summary };
  assert.equal(validDiscordPcSaveResult(structured, "best-save"), true);
  assert.equal(
    selectDiscordBestSaveWinner(summary).group.canonical_candidate_id,
    "9007199254740993",
  );
  assert.equal(summary.best_save_winners.length, 2, "typed winner list is not mutated");

  const forged = structuredClone(structured);
  forged.summary.portfolio_count = 2;
  assert.equal(validDiscordPcSaveResult(forged, "best-save"), false);

  const nestedForgery = structuredClone(structured);
  nestedForgery.summary.best_save_winners[0].group.alternative_cursor = "forged";
  assert.equal(validDiscordPcSaveResult(nestedForgery, "best-save"), false);

  const missingWitness = structuredClone(structured);
  delete missingWitness.summary.best_save_canonical_winner;
  assert.equal(validDiscordPcSaveResult(missingWitness, "best-save"), false);

  const mismatchedWitness = structuredClone(structured);
  mismatchedWitness.summary.best_save_canonical_winner = structuredClone(winners[1]);
  assert.equal(validDiscordPcSaveResult(mismatchedWitness, "best-save"), false);
});

test("Discord rejects non-canonical or out-of-range candidate-id transports", () => {
  for (const candidateId of [0, "0", "01", "-1", "18446744073709551616"]) {
    const structured = {
      kind: "pc-save-groups.v2",
      summary: {
        save_contract: "pc-save-groups.v2",
        save_pc_probability: 1,
        save_groups: [group(candidateId, 1, 1)],
      },
    };
    structured.summary.save_groups[0].canonical_candidate_id = candidateId;
    structured.summary.save_groups[0].witnesses[0].candidate_id = candidateId;
    assert.equal(validDiscordPcSaveResult(structured, "saves"), false);
  }
});

test("Discord requires group cardinality and canonical witness identity to agree", () => {
  const structured = {
    kind: "pc-save-groups.v2",
    summary: {
      save_contract: "pc-save-groups.v2",
      save_pc_probability: 0.5,
      save_groups: [group(7, 0.25, 0.5)],
    },
  };
  assert.equal(validDiscordPcSaveResult(structured, "saves"), true);

  const countMismatch = structuredClone(structured);
  countMismatch.summary.save_groups[0].successful_pattern_count = 2;
  assert.equal(validDiscordPcSaveResult(countMismatch, "saves"), false);

  const canonicalMismatch = structuredClone(structured);
  canonicalMismatch.summary.save_groups[0].canonical_candidate_id = "8";
  assert.equal(validDiscordPcSaveResult(canonicalMismatch, "saves"), false);

  const impossibleProbability = structuredClone(structured);
  impossibleProbability.summary.save_groups[0].unconditional_probability = 0.75;
  assert.equal(validDiscordPcSaveResult(impossibleProbability, "saves"), false);

  const empty = structuredClone(structured);
  empty.summary.save_groups = [];
  assert.equal(validDiscordPcSaveResult(empty, "saves"), false);
});

test("Discord canonical best-save display fails closed before reading an invalid supplied ID", () => {
  const winner = {
    weighted_total: 6,
    balanced_jl_count: 0,
    exact_group_probability: 1,
    group: group("not-a-decimal", 1, 1),
  };
  const summary = {
    best_save_contract: "pc-best-save.v2",
    best_save_schema: "clearra-save-v1",
    best_save_probability_basis: "whole-universe-unconditional",
    best_save_pc_probability: 1,
    best_save_canonical_selection: "smallest-canonical-candidate-id",
    best_save_canonical_winner: structuredClone(winner),
    best_save_winners: [winner],
  };
  assert.equal(selectDiscordBestSaveWinner(summary), null);
});
