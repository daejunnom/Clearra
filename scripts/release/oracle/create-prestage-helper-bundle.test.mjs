import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { cp, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

import {
  ORACLE_PRESTAGE_HELPER_SCHEMA,
  createPrestageHelperBundleManifest,
} from "./create-prestage-helper-bundle.mjs";

const REPOSITORY_ROOT = resolve(
  dirname(fileURLToPath(import.meta.url)),
  "../../..",
);
const NONCE = "a".repeat(64);
const FILES = Object.freeze([
  "apps/clearra-discord-bot/scripts/capture-oracle-rollback-authority.mjs",
  "apps/clearra-discord-bot/scripts/oracle-runtime-authority.mjs",
  "apps/clearra-discord-bot/scripts/release-tree-digest.mjs",
  "apps/clearra-discord-bot/src/job-service/runtime-identity.mjs",
]);

function git(root, ...arguments_) {
  const result = spawnSync("git", ["-C", root, ...arguments_], {
    shell: false,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
  assert.equal(result.status, 0, result.stderr);
  return result.stdout.trim();
}

async function fixture() {
  const root = await mkdtemp(join(tmpdir(), "clearra-prestage-helper-"));
  for (const path of FILES) {
    const target = join(root, ...path.split("/"));
    await mkdir(dirname(target), { recursive: true });
    await cp(join(REPOSITORY_ROOT, ...path.split("/")), target);
  }
  git(root, "init", "--initial-branch=main");
  git(root, "config", "user.name", "Clearra Test");
  git(root, "config", "user.email", "clearra-test@example.invalid");
  git(root, "add", "--", ...FILES);
  git(root, "commit", "-m", "fixture");
  return { root, sourceCommit: git(root, "rev-parse", "HEAD") };
}

test("seals one exact accepted-source four-module prestage helper closure", async () => {
  const { root, sourceCommit } = await fixture();
  try {
    const capture = await createPrestageHelperBundleManifest({
      repositoryRoot: root,
      sourceCommit,
      deploymentNonce: NONCE,
      operation: "capture-prestage-authority",
    });
    assert.equal(capture.schema_id, ORACLE_PRESTAGE_HELPER_SCHEMA);
    assert.equal(capture.source_commit, sourceCommit);
    assert.equal(capture.deployment_nonce, NONCE);
    assert.equal(capture.file_count, 4);
    assert.deepEqual(capture.files.map((entry) => entry.path), FILES);
    assert.equal(capture.files.every((entry) => entry.mode === "0644"), true);
    assert.equal(
      capture.total_size,
      capture.files.reduce((sum, entry) => sum + entry.size, 0),
    );
    assert.match(capture.bundle_sha256, /^[0-9a-f]{64}$/u);

    const cleanup = await createPrestageHelperBundleManifest({
      repositoryRoot: root,
      sourceCommit,
      deploymentNonce: NONCE,
      operation: "cleanup-prestage-backup",
    });
    assert.equal(cleanup.bundle_sha256, capture.bundle_sha256);
    assert.deepEqual(cleanup.files, capture.files);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("rejects foreign commits, dirty bytes, dynamic imports, and import-closure expansion", async () => {
  const { root, sourceCommit } = await fixture();
  const capturePath = join(root, ...FILES[0].split("/"));
  try {
    await assert.rejects(
      createPrestageHelperBundleManifest({
        repositoryRoot: root,
        sourceCommit: "b".repeat(40),
        deploymentNonce: NONCE,
        operation: "capture-prestage-authority",
      }),
      /exact accepted Git checkout/u,
    );

    const original = await readFile(capturePath, "utf8");
    await writeFile(capturePath, `${original}\nimport("node:os");\n`, "utf8");
    await assert.rejects(
      createPrestageHelperBundleManifest({
        repositoryRoot: root,
        sourceCommit,
        deploymentNonce: NONCE,
        operation: "capture-prestage-authority",
      }),
      /dynamic dependency/u,
    );

    await writeFile(capturePath, `${original}\nimport { arch } from "node:os";\n`, "utf8");
    await assert.rejects(
      createPrestageHelperBundleManifest({
        repositoryRoot: root,
        sourceCommit,
        deploymentNonce: NONCE,
        operation: "capture-prestage-authority",
      }),
      /import closure drifted/u,
    );

    await writeFile(capturePath, original.replace("oracle_rollback_capture=failed", "drifted"), "utf8");
    await assert.rejects(
      createPrestageHelperBundleManifest({
        repositoryRoot: root,
        sourceCommit,
        deploymentNonce: NONCE,
        operation: "capture-prestage-authority",
      }),
      /differs from the accepted checkout/u,
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
