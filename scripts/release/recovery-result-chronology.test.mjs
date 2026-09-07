import assert from "node:assert/strict";
import test from "node:test";
import { validateRecoveryResultChronology } from "./recovery-result-chronology.mjs";

function observed(overrides = {}) {
  return {
    resolutionCreatedAt: "2026-09-07T15:14:33Z",
    resultCreatedAt: "2026-09-07T15:18:02Z",
    recoveredAt: "2026-09-07T15:18:02.034Z",
    recoveryStartedAt: "2026-09-07T15:14:13Z",
    recoveryUpdatedAt: "2026-09-07T15:18:06Z",
    ...overrides,
  };
}

test("accepts the observed verified recovery without rewriting any source timestamp", () => {
  const input = Object.freeze(observed());
  const original = JSON.stringify(input);
  assert.doesNotThrow(() => validateRecoveryResultChronology(input));
  assert.equal(JSON.stringify(input), original);
});

for (const fraction of ["000", "001", "034", "999"]) {
  test(`whole-second artifact metadata admits only its reported second: .${fraction}`, () => {
    assert.doesNotThrow(() => validateRecoveryResultChronology(observed({
      recoveredAt: `2026-09-07T15:18:02.${fraction}Z`,
    })));
  });
}

test("the very next millisecond outside the reported second is rejected", () => {
  assert.throws(() => validateRecoveryResultChronology(observed({
    recoveredAt: "2026-09-07T15:18:03.000Z",
  })), /check=recovered-after-artifact-precision/u);
});

test("an explicitly .000 artifact timestamp never gains a second of tolerance", () => {
  assert.throws(() => validateRecoveryResultChronology(observed({
    resultCreatedAt: "2026-09-07T15:18:02.000Z",
  })), /check=recovered-after-artifact-precision/u);
});

test("millisecond artifact equality passes but one millisecond later fails", () => {
  assert.doesNotThrow(() => validateRecoveryResultChronology(observed({
    resultCreatedAt: "2026-09-07T15:18:02.034Z",
  })));
  assert.throws(() => validateRecoveryResultChronology(observed({
    resultCreatedAt: "2026-09-07T15:18:02.033Z",
  })), /check=recovered-after-artifact-precision/u);
});

test("a result from an earlier second remains acceptable within the original run bounds", () => {
  assert.doesNotThrow(() => validateRecoveryResultChronology(observed({
    recoveredAt: "2026-09-07T15:18:01.999Z",
  })));
});

test("the completed-run upper bound remains exact even within the artifact second", () => {
  for (const recoveryUpdatedAt of ["2026-09-07T15:18:02Z", "2026-09-07T15:18:02.033Z"]) {
    assert.throws(() => validateRecoveryResultChronology(observed({ recoveryUpdatedAt })),
      /check=recovered-after-run/u);
  }
  assert.doesNotThrow(() => validateRecoveryResultChronology(observed({
    recoveryUpdatedAt: "2026-09-07T15:18:02.034Z",
  })));
});

test("the original resolution and recovery-start lower bounds stay exact", () => {
  assert.throws(() => validateRecoveryResultChronology(observed({
    resolutionCreatedAt: "2026-09-07T15:18:03Z",
  })), /check=artifact-before-resolution/u);
  assert.throws(() => validateRecoveryResultChronology(observed({
    recoveredAt: "2026-09-07T15:14:32.999Z",
  })), /check=recovered-before-resolution/u);
  assert.throws(() => validateRecoveryResultChronology(observed({
    recoveryStartedAt: "2026-09-07T15:18:02.035Z",
  })), /check=recovered-before-run/u);
});

for (const field of Object.keys(observed())) {
  test(`rejects malformed or noncanonical ${field} without echoing its content`, () => {
    for (const value of [undefined, null, 0, {}, "fixture-secret", "2026-02-30T00:00:00Z",
      "2026-09-07T15:18:02.03Z", "2026-09-07T15:18:02+00:00"]) {
      assert.throws(() => validateRecoveryResultChronology(observed({ [field]: value })), (error) => {
        assert.match(error.message, /check=invalid-/u);
        assert.doesNotMatch(error.message, /fixture-secret|2026-02-30/u);
        return true;
      });
    }
  });
}
