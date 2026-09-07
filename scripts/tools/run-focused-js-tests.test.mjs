import assert from "node:assert/strict";
import { mkdir, mkdtemp, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, before, test } from "node:test";
import { EventEmitter } from "node:events";

import {
  buildFocusedTestCommandGroups,
  resolveFocusedTestSelection,
  runFocusedTests,
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

function focusedMockSpawn(outcomes, calls) {
  return (command, args, options) => {
    const outcome = outcomes[calls.length];
    calls.push({ command, args, options });
    const child = new EventEmitter();
    queueMicrotask(() => outcome.error
      ? child.emit('error', outcome.error)
      : child.emit('exit', outcome.code, outcome.signal ?? null));
    return child;
  };
}

test('focused Node assertion failure does not skip independent TypeScript contracts', async () => {
  const calls = [];
  await assert.rejects(runFocusedTests(['suite/alpha.test.mjs', 'suite/model.contract.ts'], {
    repositoryRoot,
    spawnImplementation: focusedMockSpawn([{ code: 1 }, { code: 0 }], calls),
  }), /failed in 1 group\(s\).*node-test/u);
  assert.equal(calls.length, 2);
  assert.ok(calls[1].args.includes('suite/model.contract.ts'));
  assert.ok(calls.every((call) => call.options.shell === false && call.options.windowsHide === true));
});

test('focused tests retain all independent group failures and never retry', async () => {
  const calls = [];
  await assert.rejects(runFocusedTests(['suite/alpha.test.mjs', 'suite/model.contract.ts'], {
    repositoryRoot,
    spawnImplementation: focusedMockSpawn([{ code: 1 }, { code: 2 }], calls),
  }), /failed in 2 group\(s\).*node-test.*typescript-contract/u);
  assert.equal(calls.length, 2);
});

test('focused cancellation and shared runner errors stop subsequent groups', async () => {
  for (const outcome of [{ code: null, signal: 'SIGTERM' }, { code: null },
    { code: 0, signal: 'SIGINT' }, { error: new Error('shared spawn failure') }]) {
    const calls = [];
    await assert.rejects(runFocusedTests(['suite/alpha.test.mjs', 'suite/model.contract.ts'], {
      repositoryRoot,
      spawnImplementation: focusedMockSpawn([outcome], calls),
    }), /interrupted|shared spawn failure/u);
    assert.equal(calls.length, 1);
  }
});
