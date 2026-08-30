import assert from "node:assert/strict";
import test from "node:test";

import {
  discordPcScoreMinimalsResultProjection,
  discordPcScoreMinimalsSummaryLines,
  validDiscordPcScoreMinimalsResult,
} from "../src/discord/pc-score-minimals-result.mjs";

test("score-minimals accepts only the score-only closed result and emits one canonical ID", () => {
  const result = validResult();
  assert.equal(validDiscordPcScoreMinimalsResult(result), true);
  assert.deepEqual(discordPcScoreMinimalsResultProjection(result), {
    canonicalCandidateId: "2",
    canonicalSolutionKey: "pc:solution:02",
    scoreEquality: "score-only",
    attackRole: "informational-only",
  });

  const lines = discordPcScoreMinimalsSummaryLines(result, "en");
  assert.deepEqual(lines, [
    "Canonical candidate ID: 2",
    "Canonical solution key: pc:solution:02",
    "Score equality: score-only",
    "Attack role: informational-only",
  ]);
  const rendered = lines.join("\n");
  assert.equal((rendered.match(/candidate ID/gu) ?? []).length, 1);
  assert.doesNotMatch(rendered, /tie|alternative|cursor|page/iu);
});

test("attack observations cannot change score-minimals selection or membership metadata", () => {
  const lowAttack = validResult();
  lowAttack.summary.score_best_attack = 0;
  const highAttack = structuredClone(lowAttack);
  highAttack.summary.score_best_attack = 999_999;

  assert.deepEqual(
    discordPcScoreMinimalsResultProjection(lowAttack),
    discordPcScoreMinimalsResultProjection(highAttack),
  );
  assert.equal(validDiscordPcScoreMinimalsResult({
    ...lowAttack,
    summary: {
      ...lowAttack.summary,
      score_minimals_attack_role: "score-and-attack",
    },
  }), false);
  assert.equal(validDiscordPcScoreMinimalsResult({
    ...lowAttack,
    summary: {
      ...lowAttack.summary,
      score_minimals_score_equality: "score-and-attack",
    },
  }), false);
});

test("score-minimals validates a complete canonical portfolio but publishes only its smallest ID", () => {
  const result = validResult();
  result.summary.optimal_cardinality = "2";
  result.summary.members.push({
    candidate_id: "9",
    normalized_solution_key: "pc:solution:09",
  });

  assert.equal(validDiscordPcScoreMinimalsResult(result), true);
  assert.deepEqual(discordPcScoreMinimalsResultProjection(result), {
    canonicalCandidateId: "2",
    canonicalSolutionKey: "pc:solution:02",
    scoreEquality: "score-only",
    attackRole: "informational-only",
  });
  assert.doesNotMatch(
    discordPcScoreMinimalsSummaryLines(result, "en").join("\n"),
    /pc:solution:09|\b9\b/u,
  );
});

test("score-minimals accepts an open canonical enumeration only with an unknown total", () => {
  const result = validResult();
  result.summary.enumeration_complete = false;
  result.summary.total_alternative_count = null;

  assert.equal(validDiscordPcScoreMinimalsResult(result), true);
  assert.deepEqual(discordPcScoreMinimalsResultProjection(result), {
    canonicalCandidateId: "2",
    canonicalSolutionKey: "pc:solution:02",
    scoreEquality: "score-only",
    attackRole: "informational-only",
  });
});

test("score-minimals rejects every Discord tie, page, cursor, or second-candidate projection", () => {
  for (const [key, value] of [
    ["tie_cursor", "opaque"],
    ["portfolio_alternative_page", { member_candidate_ids: ["2", "9"] }],
    ["known_alternative_count", 2],
    ["total_alternative_count", 2],
    ["candidate_ids", ["2", "9"]],
    ["selected_score_candidate_ids", ["2", "9"]],
    ["other_solution_key", "pc:solution:09"],
  ]) {
    const result = validResult();
    result.summary.details = { [key]: value };
    assert.equal(
      validDiscordPcScoreMinimalsResult(result),
      false,
      `${key} must remain unavailable on Discord`,
    );
  }
});

test("score-minimals fails closed on incomplete evidence and forged canonical fields", () => {
  for (const mutate of [
    (result) => { result.kind = "pc-scenario"; },
    (result) => { result.contract.command.kind = "pc-score-summary.v2"; },
    (result) => { result.resource_report.count_complete = false; },
    (result) => { result.resource_report.truncated = true; },
    (result) => { result.summary.alternative_index = "2"; },
    (result) => { result.summary.member_page_number = "2"; },
    (result) => { result.summary.page_handle_available = false; },
    (result) => { result.summary.page_handle_available = "true"; },
    (result) => { result.summary.enumeration_complete = false; },
    (result) => { result.summary.total_alternative_count = null; },
    (result) => { result.summary.total_alternative_count = "2"; },
    (result) => {
      result.summary.enumeration_complete = false;
      result.summary.total_alternative_count = "2";
    },
    (result) => { result.summary.members[0].candidate_id = "3"; },
    (result) => { result.summary.score_minimals_canonical_candidate_id = "0"; },
    (result) => { result.summary.score_minimals_canonical_candidate_id = "02"; },
    (result) => { result.summary.score_minimals_canonical_solution_key = "@everyone"; },
  ]) {
    const result = validResult();
    mutate(result);
    assert.equal(validDiscordPcScoreMinimalsResult(result), false);
  }
});

function validResult() {
  return {
    kind: "pc-score-portfolio.v2",
    contract: {
      command: { kind: "pc-score-portfolio.v2" },
      pc: { scoring: { score_equality: "score-only", attack_role: "informational-only" } },
    },
    summary: {
      capability_id: "pc.score-minimals",
      result_contract: "pc-score-portfolio.v2",
      payload_kind: "coverage-portfolio",
      set_contract: "portfolio-alternative-set.v1",
      page_contract: "portfolio-alternative-page.v1",
      member_page_contract: "portfolio-member-page.v1",
      set_identity_sha256: "a".repeat(64),
      candidate_map_sha256: "b".repeat(64),
      alternative_index: "1",
      optimal_cardinality: "1",
      known_alternative_count: "1",
      total_alternative_count: "1",
      enumeration_complete: true,
      member_page_number: "1",
      total_member_pages: "1",
      members: [{
        candidate_id: "2",
        normalized_solution_key: "pc:solution:02",
      }],
      page_handle_available: true,
      score_minimals_contract: "pc-score-portfolio.v2",
      score_minimals_score_equality: "score-only",
      score_minimals_attack_role: "informational-only",
      score_minimals_canonical_selection: "smallest-canonical-candidate-id",
      score_minimals_canonical_candidate_id: "2",
      score_minimals_canonical_solution_key: "pc:solution:02",
      score_best_score: 40_000,
      score_best_attack: 11,
    },
    resource_report: {
      probability_complete: true,
      count_complete: true,
      truncated: false,
      truncation_reason: null,
      count_truncated_reason: null,
      renormalized: false,
    },
  };
}
