import assert from "node:assert/strict";
import { mkdir, mkdtemp, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, before, test } from "node:test";

import {
  buildFocusedTestCommandGroups,
  resolveFocusedTestSelection,
} from "./run-focused-js-tests.mjs";

let repositoryRoot;
let symlinkFixturesAvailable = false;

before(async () => {
  repositoryRoot = await mkdtemp(join(tmpdir(), "clearra-focused-tests-"));
  await mkdir(join(repositoryRoot, "suite"), { recursive: true });
  await writeFile(join(repositoryRoot, "suite", "beta.test.mjs"), "");
  await writeFile(join(repositoryRoot, "suite", "alpha.test.mjs"), "");
  await writeFile(join(repositoryRoot, "suite", "model.contract.ts"), "");
  await mkdir(join(repositoryRoot, "suite", "directory.test.mjs"));
  await writeFile(
    join(repositoryRoot, "--test-reporter-destination=owned.test.mjs"),
    "",
  );
  try {
    await symlink(
      join(repositoryRoot, "suite", "alpha.test.mjs"),
      join(repositoryRoot, "linked.test.mjs"),
      "file",
    );
    await symlink(
      join(repositoryRoot, "suite"),
      join(repositoryRoot, "linked-suite"),
      process.platform === "win32" ? "junction" : "dir",
    );
    symlinkFixturesAvailable = true;
  } catch (error) {
    if (error?.code !== "EPERM") throw error;
  }
});

after(async () => {
  await rm(repositoryRoot, { force: true, recursive: true });
});

test("selects only explicit files and groups each runner once", async () => {
  const selection = await resolveFocusedTestSelection(
    [
      "suite/beta.test.mjs",
      "suite/model.contract.ts",
      "suite\\alpha.test.mjs",
    ],
    { repositoryRoot },
  );

  assert.deepEqual(selection.nodeTests, [
    "suite/alpha.test.mjs",
    "suite/beta.test.mjs",
  ]);
  assert.deepEqual(selection.typescriptContracts, [
    "suite/model.contract.ts",
  ]);
  assert.deepEqual(
    buildFocusedTestCommandGroups(selection).map(({ label, args }) => ({
      label,
      args,
    })),
    [
      {
        label: "node-test",
        args: [
          "--test",
          "--",
          "suite/alpha.test.mjs",
          "suite/beta.test.mjs",
        ],
      },
      {
        label: "typescript-contract",
        args: [
          "scripts/tools/run-typescript-contracts.mjs",
          "suite/model.contract.ts",
        ],
      },
    ],
  );
});

test("requires at least one explicit file", async () => {
  await assert.rejects(
    resolveFocusedTestSelection([], { repositoryRoot }),
    /at least one explicit repository-relative/u,
  );
});

test("rejects absolute paths, traversal, globs, and noncanonical paths", async () => {
  for (const input of [
    "/suite/alpha.test.mjs",
    "C:\\repo\\suite\\alpha.test.mjs",
    "../suite/alpha.test.mjs",
    "suite/*.test.mjs",
    "suite/!(alpha).test.mjs",
    "suite/@(alpha|beta).test.mjs",
    "suite/+(alpha|beta).test.mjs",
    "./suite/alpha.test.mjs",
    "--test-reporter-destination=owned.test.mjs",
  ]) {
    await assert.rejects(
      resolveFocusedTestSelection([input], { repositoryRoot }),
    );
  }
});

test("rejects file and directory symlink aliases", async (context) => {
  if (!symlinkFixturesAvailable) {
    context.skip("this host does not permit test symlink creation");
    return;
  }
  await assert.rejects(
    resolveFocusedTestSelection(["linked.test.mjs"], { repositoryRoot }),
    /symbolic link/u,
  );
  await assert.rejects(
    resolveFocusedTestSelection(["linked-suite/alpha.test.mjs"], {
      repositoryRoot,
    }),
    /symbolic-link directory/u,
  );
});

test("rejects heavy and secret locations before reading them", async () => {
  for (const input of [
    "node_modules/owned.test.mjs",
    "dist/owned.test.mjs",
    "build/owned.test.mjs",
    ".cache/owned.test.mjs",
    "credentials/owned.test.mjs",
    "secret/owned.test.mjs",
    "api-keys/owned.test.mjs",
    ".ssh/owned.test.mjs",
    ".env.fixture.test.mjs",
    "id_ed25519.test.mjs",
    "id_ed25519_work.test.mjs",
    "credentials.test.mjs",
    "api-key.test.mjs",
  ]) {
    await assert.rejects(
      resolveFocusedTestSelection([input], { repositoryRoot }),
      /(heavy|secret|SSH credential)/u,
    );
  }
});

test("rejects unsupported suffixes, directories, missing files, and duplicates", async () => {
  await assert.rejects(
    resolveFocusedTestSelection(["suite/model.ts"], { repositoryRoot }),
    /must end in/u,
  );
  await assert.rejects(
    resolveFocusedTestSelection(["suite/directory.test.mjs"], {
      repositoryRoot,
    }),
    /regular file/u,
  );
  await assert.rejects(
    resolveFocusedTestSelection(["suite/missing.test.mjs"], {
      repositoryRoot,
    }),
    /does not exist/u,
  );
  await assert.rejects(
    resolveFocusedTestSelection(
      ["suite/alpha.test.mjs", "suite\\alpha.test.mjs"],
      { repositoryRoot },
    ),
    /duplicated/u,
  );
});
