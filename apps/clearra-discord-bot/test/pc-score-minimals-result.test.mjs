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
