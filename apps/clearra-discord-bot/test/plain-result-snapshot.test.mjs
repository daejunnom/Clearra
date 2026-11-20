import assert from "node:assert/strict";
import test from "node:test";
import { projectDiscordBuildV2Result } from "../src/discord/build-v2-result.mjs";
import { discordLegacyRouteProjection } from "../src/discord/capability-registry.mjs";
import {
  clonePlain,
  deepFreeze,
  plainObject,
  stripAttackFields,
} from "../src/discord/plain-result-snapshot.mjs";

test("Discord snapshots only enumerable own string keys without assigning through the prototype", () => {
  const value = Object.assign(Object.create({ inherited: true }), {
    rows: [Object.assign(Object.create(null), { candidate: "1" })],
  });
  Object.defineProperty(value, "__proto__", {
    enumerable: true,
    value: { ownKey: true },
  });
  Object.defineProperty(value, "hidden", { value: true });
  value[Symbol("hidden")] = true;

  assert.equal(plainObject(value), true);
  assert.equal(plainObject(Object.create(null)), true);
  assert.equal(plainObject(null), false);
  assert.equal(plainObject(value.rows), false);
  const snapshot = clonePlain(value);
  assert.deepEqual(Object.keys(snapshot), ["rows", "__proto__"]);
  assert.equal(Object.getPrototypeOf(snapshot), Object.prototype);
  assert.equal(Object.hasOwn(snapshot, "__proto__"), true);
  assert.deepEqual(snapshot.__proto__, { ownKey: true });
  assert.notEqual(snapshot.rows, value.rows);
  assert.notEqual(snapshot.rows[0], value.rows[0]);
  assert.deepEqual(snapshot.rows, [{ candidate: "1" }]);

  assert.equal(deepFreeze(snapshot), snapshot);
  assert.equal(Object.isFrozen(snapshot.rows), true);
  assert.equal(Object.isFrozen(snapshot.rows[0]), true);
  assert.equal(Object.isFrozen(snapshot.__proto__), true);
  assert.equal(Object.isFrozen(value.rows[0]), false);
});

test("Discord snapshot freeze preserves the already frozen subtree boundary", () => {
  const child = { candidate: "2" };
  const frozen = Object.freeze({ child });
  const result = { frozen };
  deepFreeze(result);
  assert.equal(Object.isFrozen(result), true);
  assert.equal(Object.isFrozen(frozen), true);
  assert.equal(Object.isFrozen(child), false);
});

test("strict Discord attack projection removes matching keys recursively without mutating its input", () => {
  const source = {
    summary: {
      score_minimals_attack_role: "informational-only",
      nested: [{ attack: "3", AttackBasis: "info", score: "100" }, "attack"],
    },
    preserved: null,
  };
  assert.deepEqual(stripAttackFields(source), {
    summary: { nested: [{ score: "100" }, "attack"] },
    preserved: null,
  });
  assert.equal(source.summary.score_minimals_attack_role, "informational-only");
  assert.equal(source.summary.nested[0].attack, "3");
});

test("ordinary Build v2 output is an independently frozen, attack-free snapshot", () => {
  const contract = "build-supplied-coverage.v1";
  const source = {
    kind: contract,
    contract: { command: { kind: contract } },
    summary: {
      capability_id: "build.evaluate.cover",
      result_contract: contract,
      payload_kind: "candidate-family",
      completeness: { complete: true },
      candidates: [{ candidate_key: "existing", nested: { attack: "3", score: "100" } }],
    },
  };
  const projected = projectDiscordBuildV2Result(source);
  assert.deepEqual(projected.summary.candidates, [
    { candidate_key: "existing", nested: { score: "100" } },
  ]);
  assert.equal(Object.isFrozen(projected.summary.candidates[0].nested), true);
  assert.equal(Object.isFrozen(source.summary.candidates[0].nested), false);
  assert.equal(source.summary.candidates[0].nested.attack, "3");
  assert.deepEqual(projectDiscordBuildV2Result(projected), projected);
});

test("Discord legacy registry projections keep independent frozen records and argv arrays", () => {
  const first = discordLegacyRouteProjection();
  const second = discordLegacyRouteProjection();
  const index = first.findIndex((row) => Array.isArray(row.argv_prefix));
  assert.ok(index >= 0);
  assert.notEqual(first, second);
  assert.deepEqual(first, second);
  assert.notEqual(first[index], second[index]);
  assert.notEqual(first[index].argv_prefix, second[index].argv_prefix);
  assert.equal(Object.isFrozen(first), true);
  assert.equal(Object.isFrozen(first[index]), true);
  assert.equal(Object.isFrozen(first[index].argv_prefix), true);
});
