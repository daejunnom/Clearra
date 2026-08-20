import assert from "node:assert/strict";
import test from "node:test";

import { verifyAcceptedSource } from "../scripts/verify-accepted-source.mjs";
import { currentRuntimeIdentityForCommit } from "../src/job-service/runtime-identity.mjs";

const SOURCE_COMMIT = "7".repeat(40);
const REPOSITORY = "daejunnom/Clearra";
const TEST_RUNTIME_IDENTITY = currentRuntimeIdentityForCommit(SOURCE_COMMIT);

test("accepted-source preflight requires exact main, canonical acceptance, and active runtime", async () => {
  const calls = [];
  const result = await verifyAcceptedSource(
    {
      sourceCommit: SOURCE_COMMIT,
      repository: REPOSITORY,
      activeHealthUrl: "https://clearra.example.run.app/ignored",
    },
    {
      run(command, arguments_) {
        calls.push([command, arguments_]);
        if (command === "git" && arguments_[0] === "fetch") return "";
        if (command === "git") return `${SOURCE_COMMIT}\n`;
        return JSON.stringify({ workflow_runs: [{ head_sha: SOURCE_COMMIT }] });
      },
      async fetchImpl(url, options) {
        calls.push(["fetch", url.href, options]);
        return {
          ok: true,
          async json() {
            return {
              runtime: TEST_RUNTIME_IDENTITY,
            };
          },
        };
      },
    },
  );

  assert.deepEqual(result, { repository: REPOSITORY, sourceCommit: SOURCE_COMMIT });
  assert.deepEqual(calls[0], [
    "git",
    ["fetch", "--no-tags", "--depth=1", "origin", "main"],
  ]);
  assert.deepEqual(calls[3], [
    "gh",
    [
      "api",
      "--method",
      "GET",
      `repos/${REPOSITORY}/actions/workflows/release-cli.yml/runs`,
      "-f",
      "event=workflow_dispatch",
      "-f",
      "status=success",
      "-f",
      `head_sha=${SOURCE_COMMIT}`,
      "-f",
      "per_page=1",
    ],
  ]);
  assert.equal(calls[4][0], "fetch");
  assert.equal(calls[4][1], "https://clearra.example.run.app/health");
  assert.equal(calls[4][2].method, "GET");
});

test("accepted-source preflight fails closed on stale source, missing acceptance, or runtime drift", async () => {
  await assert.rejects(
    verifyAcceptedSource(
      { sourceCommit: SOURCE_COMMIT, repository: REPOSITORY },
      {
        run(command, arguments_) {
          if (command === "git" && arguments_[0] === "fetch") return "";
          if (command === "git" && arguments_.at(-1) === "origin/main") {
            return `${"8".repeat(40)}\n`;
          }
          return `${SOURCE_COMMIT}\n`;
        },
      },
    ),
    /exact current origin\/main/,
  );

  await assert.rejects(
    verifyAcceptedSource(
      { sourceCommit: SOURCE_COMMIT, repository: REPOSITORY },
      {
        run(command, arguments_) {
          if (command === "git" && arguments_[0] === "fetch") return "";
          if (command === "git") return `${SOURCE_COMMIT}\n`;
          return JSON.stringify({ workflow_runs: [] });
        },
      },
    ),
    /no successful canonical acceptance/,
  );

  await assert.rejects(
    verifyAcceptedSource(
      {
        sourceCommit: SOURCE_COMMIT,
        repository: REPOSITORY,
        activeHealthUrl: "https://clearra.example.run.app",
      },
      {
        run(command, arguments_) {
          if (command === "git" && arguments_[0] === "fetch") return "";
          if (command === "git") return `${SOURCE_COMMIT}\n`;
          return JSON.stringify({ workflow_runs: [{}] });
        },
        async fetchImpl() {
          return {
            ok: true,
            async json() {
              return {
                runtime: {
                  ...TEST_RUNTIME_IDENTITY,
                  engineBuildId: "8".repeat(40),
                },
              };
            },
          };
        },
      },
    ),
    /active runtime identity/,
  );
});
