import assert from "node:assert/strict";
import test from "node:test";

import {
  ClearraExecutionRouter,
  explicitTablebaseRequested,
} from "../src/clearra/execution-router.mjs";

function executor(label, calls) {
  return {
    execute(arguments_, options) {
      calls.push({ label, arguments_: [...arguments_], options });
      return Promise.resolve(label);
    },
  };
}

test("explicit tablebase work uses only the local authority", async () => {
  const calls = [];
  const router = new ClearraExecutionRouter(
    executor("cloud", calls),
    executor("oracle-tablebase", calls),
  );

  assert.equal(
    await router.execute(["pc", "minimals", "--tablebase"], { jobId: "one" }),
    "oracle-tablebase",
  );
  assert.deepEqual(calls.map(({ label }) => label), ["oracle-tablebase"]);
});

test("tablebase miss or failure cannot fall through to Cloud Run", async () => {
  const calls = [];
  const tablebase = {
    execute() {
      calls.push("oracle-tablebase");
      return Promise.reject(new Error("pc4_online_not_found"));
    },
  };
  const router = new ClearraExecutionRouter(executor("cloud", calls), tablebase);

  await assert.rejects(
    router.execute(["pc", "path", "--tablebase"]),
    /pc4_online_not_found/u,
  );
  assert.deepEqual(calls, ["oracle-tablebase"]);
});

test("ordinary and explicit no-tablebase work retains the primary authority", async () => {
  const calls = [];
  const router = new ClearraExecutionRouter(
    executor("cloud", calls),
    executor("oracle-tablebase", calls),
  );

  assert.equal(await router.execute(["pc", "path", "--no-tablebase"]), "cloud");
  assert.equal(await router.execute(["build", "path"]), "cloud");
  assert.deepEqual(calls.map(({ label }) => label), ["cloud", "cloud"]);
});

test("unconfigured tablebase authority fails before invoking the primary", () => {
  const calls = [];
  const router = new ClearraExecutionRouter(executor("cloud", calls));
  assert.throws(
    () => router.execute(["pc", "score", "--tablebase"]),
    /tablebase service is unavailable/u,
  );
  assert.deepEqual(calls, []);
});

test("tablebase intent recognizes the canonical aliases deterministically", () => {
  assert.equal(explicitTablebaseRequested(["pc", "path", "--tablebase"]), true);
  assert.equal(explicitTablebaseRequested(["pc", "path", "--tb"]), true);
  assert.equal(explicitTablebaseRequested(["pc", "path", "--no-tablebase"]), false);
  assert.equal(explicitTablebaseRequested(["pc", "path", "--tablebase", "--no-tb"]), false);
});
