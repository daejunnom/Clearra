import assert from "node:assert/strict";
import test from "node:test";

import { Clearrabot } from "../src/bot.mjs";

const bot = new Clearrabot({}, { maxConcurrentSearches: 1 }, {
  executor: { async execute() { throw new Error("not used"); } },
});

const fixed = {
  kind: "boundary-recovery",
  summary: {
    contract: "boundary-recovery.v1",
    status: "no-path-within-declared-scope",
    knowledge_basis: "full-fixed-queue",
    placement_role_scope: "occupancy-only",
    normal_states: 12,
    recovery_states: 34,
    borrowed_stage_two_count: 0,
    stage_one_checkpoint_step: null,
    checkpoint_is_pc: null,
    steps: [],
  },
};

async function render(payload, locale = "en") {
  const message = await bot.buildResultMessage({
    exitCode: 0, stderr: "", stdout: JSON.stringify(payload),
  }, false, { resultKind: "boundary-recovery", locale });
  return message.payload.content;
}

test("fixed recovery reports the outcome without a fictitious CTK3 page", async () => {
  const content = await render(fixed);
  assert.match(content, /No path within the declared scope/u);
  assert.match(content, /Normal \+ recovery states: 12 \+ 34/u);
  assert.doesNotMatch(content, /CTK3 pages/u);
  assert.doesNotMatch(content, /partial result/u);
});

test("incomplete fixed search never presents no-path as a completed proof", async () => {
  const content = await render({
    ...fixed, summary: { ...fixed.summary, status: "incomplete" },
  });
  assert.match(content, /partial result/u);
  assert.match(content, /no impossibility claim/u);
});

test("pattern recovery displays bounded weighted outcomes and unresolved probability", async () => {
  const payload = {
    kind: "boundary-recovery",
    summary: {
      contract: "boundary-recovery.v1",
      status: "population-incomplete",
      knowledge_basis: "full-pattern-universe",
      placement_role_scope: "bag-piece-exact-lock-time",
      complete: false,
      evaluated_pattern_count: 2,
      total_possible_pattern_count: "5040",
      normal_probability: "0.10000000000000000",
      pc_preserving_recovery_probability: "0.01000000000000000",
      non_pc_recovery_probability: "0.02000000000000000",
      additional_recovery_probability: "0.03000000000000000",
      total_response_probability: "0.13000000000000000",
      no_path_probability: "0.00000000000000000",
      unknown_probability: "0.87000000000000000",
    },
  };
  const content = await render(payload, "ko");
  assert.match(content, /평가한 \/ 전체 가능 패턴: 2 \/ 5040/u);
  assert.match(content, /추가 리커버리 확률: 3%/u);
  assert.match(content, /미확정 확률: 87%/u);
  assert.doesNotMatch(content, /CTK3/u);
  assert.match(await render({
    ...payload, summary: { ...payload.summary, unknown_probability: undefined },
  }), /inconsistent result/u);
});
