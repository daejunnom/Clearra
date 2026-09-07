import assert from "node:assert/strict";
import test from "node:test";
import { workflowRunTimestampOrderIsValid } from "./workflow-run-timestamps.mjs";

const at = Date.parse("2026-09-07T06:10:40Z");

test("accepts the observed one-second skew from recovery 34089672723 attempt 1", () => {
  const createdAt = Date.parse("2026-09-07T06:10:41Z");
  const startedAt = Date.parse("2026-09-07T06:10:40Z");
  const updatedAt = Date.parse("2026-09-07T06:30:26Z");
  assert.equal(workflowRunTimestampOrderIsValid(createdAt, startedAt, updatedAt), true);
  assert.equal(startedAt, at, "chronology inputs are never clamped or restamped");
});

test("accepts ordered equal timestamps and a later rerun start", () => {
  assert.equal(workflowRunTimestampOrderIsValid(at, at, at), true);
  assert.equal(workflowRunTimestampOrderIsValid(at, at + 60_000, at + 120_000), true);
});

test("rejects a creation/start inversion beyond the narrow metadata allowance", () => {
  assert.equal(workflowRunTimestampOrderIsValid(at + 1_001, at, at + 60_000), false);
  assert.equal(workflowRunTimestampOrderIsValid(at + 2_000, at, at + 60_000), false);
});

test("does not extend update/completion bounds even by one millisecond", () => {
  assert.equal(workflowRunTimestampOrderIsValid(at, at + 1, at), false);
  assert.equal(workflowRunTimestampOrderIsValid(at + 1, at, at), false);
  assert.equal(workflowRunTimestampOrderIsValid(at + 1_000, at, at + 999), false);
});

test("a not-yet-started run still requires ordered creation and update times", () => {
  assert.equal(workflowRunTimestampOrderIsValid(at, null, at), true);
  assert.equal(workflowRunTimestampOrderIsValid(at + 1, null, at), false);
});

test("rejects missing, non-finite, non-integer and coercible timestamps", () => {
  for (const value of [undefined, NaN, Infinity, -Infinity, "0", true, {}, 0.5]) {
    assert.equal(workflowRunTimestampOrderIsValid(value, at, at), false);
    assert.equal(workflowRunTimestampOrderIsValid(at, value, at), false);
    assert.equal(workflowRunTimestampOrderIsValid(at, at, value), false);
  }
});
