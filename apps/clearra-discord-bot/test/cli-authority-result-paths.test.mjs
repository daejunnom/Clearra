import assert from "node:assert/strict";
import test from "node:test";

import {
  assertDiscordCanonicalOnlyResult,
  ClearraJobExecutor,
} from "../src/clearra/command.mjs";
import { ClearraDirectExecutor } from "../src/clearra/direct-executor.mjs";

test("direct CLI execution keeps only the numeric-smallest score-minimals artifact", async () => {
  const serviceResult = assertDiscordCanonicalOnlyResult(successResult());
  const executor = new ClearraDirectExecutor(directConfig(), {
    runner: { async execute() { return serviceResult; } },
  });

  assertCanonicalScoreMinimalsResult(
    await executor.execute(["pc", "score-minimals"]),
  );
});

test("HTTP CLI execution keeps the same score-minimals projection after service projection", async () => {
  const serviceResult = assertDiscordCanonicalOnlyResult(successResult());
  const executor = new ClearraJobExecutor({
    endpoint: "https://jobs.example.test/jobs",
    authorizationToken: "job-token",
    createJobId: () => "score-minimals-cli-authority-1",
    fetch: async () => new Response(JSON.stringify({
      protocol: "clearra.job.v1",
      id: "score-minimals-cli-authority-1",
      state: "completed",
      result: serviceResult,
    }), {
      status: 200,
      headers: { "content-type": "application/json" },
    }),
  });

  assertCanonicalScoreMinimalsResult(
    await executor.execute(["pc", "score-minimals"]),
  );
});

function assertCanonicalScoreMinimalsResult(result) {
  const payload = JSON.parse(result.stdout);
  assert.deepEqual(payload.contract.artifacts.solution_keys, ["pc:solution:02"]);
  assert.deepEqual(payload.contract.artifacts.solution_classes, ["numeric-first"]);
  assert.deepEqual(payload.contract.artifacts.solution_probabilities, [
    { solution_key: "pc:solution:02", probability: 0.1 },
  ]);
  assert.equal(payload.summary.score_minimals_canonical_candidate_id, "2");
  assert.equal(Object.hasOwn(payload.summary, "score_best_attack"), false);
  assert.equal(Object.hasOwn(payload.summary.members[1], "informational_attack"), false);
}

function successResult() {
  return {
    exitCode: 0,
    signal: null,
    stderr: "",
    stdout: JSON.stringify({
      kind: "pc-score-portfolio.v2",
      contract: {
        command: { kind: "pc-score-portfolio.v2" },
        artifacts: {
          schema_version: "clearra.solution-data.v1",
          solution_keys: ["pc:solution:10", "pc:solution:02"],
          solution_classes: ["lexical-first", "numeric-first"],
          solution_probabilities: [
            { solution_key: "pc:solution:10", probability: 0.9 },
            { solution_key: "pc:solution:02", probability: 0.1 },
          ],
        },
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
        optimal_cardinality: "2",
        known_alternative_count: "1",
        total_alternative_count: "1",
        enumeration_complete: true,
        member_page_number: "1",
        total_member_pages: "1",
        members: [
          { candidate_id: "2", normalized_solution_key: "pc:solution:02" },
          {
            candidate_id: "10",
            normalized_solution_key: "pc:solution:10",
            informational_attack: 999,
          },
        ],
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
    }),
  };
}

function directConfig() {
  return {
    executable: "clearra",
    processLogicalProcessors: 4,
    searchWorkersPerSession: 4,
    useAllLogicalProcessors: true,
    searchTimeoutMs: 3_000,
    interactionDeadlineMs: 4_000,
    maxOutputBytes: 64 * 1024,
    terminationGraceMs: 100,
  };
}
