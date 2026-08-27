import assert from "node:assert/strict";
import {
  access,
  mkdtemp,
  readFile,
  rm,
  symlink,
  unlink,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import {
  appendFinalSourceAttemptEvent,
  initializeFinalSourceAttempt,
  materializeFinalSourceManifest,
  parseFinalSourceAttemptCliArguments,
} from "./final-source-attempt-journal.mjs";
import { validateFinalSourceRevalidation } from "./validate-final-source-revalidation.mjs";

const COMMIT = "1".repeat(40);
const TREE = "2".repeat(40);
const HASH = "a".repeat(64);

test("materializes one complete hash-chained release attempt without overwriting output", async () => {
  await withAttempt(async ({ journal, root }) => {
    for (const event of completeEvents()) {
      await appendFinalSourceAttemptEvent({ journalPath: journal, ...event });
    }
    const output = join(root, "final-source.json");
    const manifest = await materializeFinalSourceManifest({
      journalPath: journal,
      outputPath: output,
    });
    assert.equal(validateFinalSourceRevalidation(manifest, {
      expectedSourceCommit: COMMIT,
    }), true);
    assert.deepEqual(JSON.parse(await readFile(output, "utf8")), manifest);
    await assert.rejects(
      materializeFinalSourceManifest({ journalPath: journal, outputPath: output }),
      (error) => error?.code === "EEXIST",
    );
  });
});

test("rejects incomplete attempts rather than fabricating missing deployment evidence", async () => {
  await withAttempt(async ({ journal, root }) => {
    await appendFinalSourceAttemptEvent({
      journalPath: journal,
      kind: "source",
      payload: sourcePayload(),
    });
    const output = join(root, "incomplete.json");
    await assert.rejects(
      materializeFinalSourceManifest({ journalPath: journal, outputPath: output }),
      /requires 1 contracts event/u,
    );
    await assert.rejects(access(output));
  });
});

test("rejects a torn or content-tampered journal", async () => {
  await withAttempt(async ({ journal }) => {
    await appendFinalSourceAttemptEvent({
      journalPath: journal,
      kind: "source",
      payload: sourcePayload(),
    });
    const original = await readFile(journal, "utf8");
    await writeFile(journal, original.replace('"branch":"main"', '"branch":"evil"'), "utf8");
    await assert.rejects(
      materializeFinalSourceManifest({ journalPath: journal }),
      /hash differs/u,
    );
    await writeFile(journal, original.slice(0, -1), "utf8");
    await assert.rejects(
      materializeFinalSourceManifest({ journalPath: journal }),
      /empty or torn/u,
    );
  });
});

test("rejects secret-shaped fields, prior authority, and concurrent writers", async () => {
  await withAttempt(async ({ journal }) => {
    await assert.rejects(
      appendFinalSourceAttemptEvent({
        journalPath: journal,
        kind: "source",
        payload: { api_token: "forbidden" },
      }),
      /forbidden secret material/u,
    );
    await assert.rejects(
      appendFinalSourceAttemptEvent({
        journalPath: journal,
        kind: "source",
        payload: { id: "v0.7.5-release-proof" },
      }),
      /reuses a v0\.7\.5 authority/u,
    );
    const lock = `${journal}.lock`;
    await writeFile(lock, "other-writer\n", { flag: "wx", mode: 0o600 });
    try {
      await assert.rejects(
        appendFinalSourceAttemptEvent({
          journalPath: journal,
          kind: "source",
          payload: sourcePayload(),
        }),
        /concurrent writer/u,
      );
    } finally {
      await unlink(lock);
    }
  });
});

test("rejects a journal reached through a symbolic link", async (t) => {
  await withAttempt(async ({ journal, root }) => {
    const linked = join(root, "linked-attempt.jsonl");
    try {
      await symlink(journal, linked, "file");
    } catch (error) {
      if (["EPERM", "EACCES", "ENOTSUP"].includes(error?.code)) {
        t.skip(`symbolic links unavailable: ${error.code}`);
        return;
      }
      throw error;
    }
    await assert.rejects(
      materializeFinalSourceManifest({ journalPath: linked }),
      /not a regular non-link file/u,
    );
  });
});

test("CLI parser accepts only exact command-specific argument sets", () => {
  assert.deepEqual(
    parseFinalSourceAttemptCliArguments([
      "initialize",
      "--journal",
      "attempt.jsonl",
      "--attempt-id",
      "release-1",
      "--source-commit",
      COMMIT,
    ]),
    {
      command: "initialize",
      values: {
        "--journal": "attempt.jsonl",
        "--attempt-id": "release-1",
        "--source-commit": COMMIT,
      },
    },
  );
  assert.throws(
    () => parseFinalSourceAttemptCliArguments(["future", "--journal", "x"]),
    /unsupported final-source attempt command/u,
  );
  assert.throws(
    () => parseFinalSourceAttemptCliArguments([
      "append", "--journal", "x", "--kind", "source", "--kind", "contracts",
    ]),
    /duplicate final-source attempt argument/u,
  );
  assert.throws(
    () => parseFinalSourceAttemptCliArguments([
      "materialize", "--journal", "x", "--future", "y",
    ]),
    /unsupported final-source attempt argument/u,
  );
});

async function withAttempt(body) {
  const root = await mkdtemp(join(tmpdir(), "clearra-final-source-attempt-"));
  const journal = join(root, "attempt.jsonl");
  try {
    await initializeFinalSourceAttempt({
      journalPath: journal,
      attemptId: "release-v0.8.0-test",
      sourceCommit: COMMIT,
    });
    await body({ root, journal });
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}

function completeEvents() {
  return [
    event("source", sourcePayload()),
    event("contracts", {
      source_commit: COMMIT,
      product_registry_schema_id: "clearra.product-capability-registry.v1",
      product_registry_sha256: HASH,
      search_option_contract_sha256: HASH,
      legacy_alias_contract_sha256: HASH,
      ctk3_contract_sha256: HASH,
      readiness_open_count: 0,
    }),
    event("toolchains", {
      source_commit: COMMIT,
      manifest_sha256: HASH,
      rust: "rustc 1.90.0",
      node: "v24.0.0",
      wasm_bindgen: "wasm-bindgen 0.2.126",
    }),
    event("drift-audit", evidence("implementation-start", {
      phase: "implementation-start",
      status: "no-drift",
    })),
    event("drift-audit", evidence("release-freeze", {
      phase: "release-freeze",
      status: "no-drift",
    })),
    event("canonical-gate", evidence("release-acceptance", {
      status: "passed",
      readiness_open_count: 0,
    })),
    ...["native", "wasm", "desktop", "discord"].map((surface) =>
      event("surface-report", evidence(`${surface}-surface`, {
        surface,
        status: "passed",
      })),
    ),
    event("release-artifact", artifact(
      "linux-cli",
      "Clearra-CLI-v0.8.0-linux-x86_64",
      1,
    )),
    event("release-artifact", artifact(
      "windows-cli",
      "Clearra-CLI-v0.8.0-windows-x86_64.exe",
      2,
    )),
    event("release-artifact", artifact(
      "windows-gui",
      "Clearra-GUI-v0.8.0-windows-x86_64.exe",
      3,
    )),
    event("deployment-pages", {
      source_commit: COMMIT,
      deployment_id: "pages-1",
      artifact_sha256: HASH,
      status: "active",
    }),
    event("deployment-discord", {
      source_commit: COMMIT,
      image_digest: `sha256:${HASH}`,
      job_revision: "job-1",
      oracle_revision: "oracle-1",
      traffic_percent: 100,
      command_catalog_sha256: HASH,
      catalog_synced: true,
      status: "active",
    }),
    event("rollback-snapshot", evidence("rollback", { status: "captured" })),
    event("observation", {
      source_commit: COMMIT,
      started_at: "2026-08-27T00:00:00.000Z",
      ended_at: "2026-08-27T00:20:00.000Z",
      duration_seconds: 1200,
      status: "passed",
      report_sha256: HASH,
    }),
    event("tag", {
      name: "v0.8.0",
      target_commit: COMMIT,
      annotated: true,
      remote_verified: true,
    }),
    event("immutable-release", {
      tag: "v0.8.0",
      source_commit: COMMIT,
      workflow_run_id: "123",
      immutable: true,
      asset_count: 3,
      status: "published",
    }),
  ];
}

function event(kind, payload) {
  return { kind, payload };
}

function sourcePayload() {
  return {
    commit: COMMIT,
    tree: TREE,
    branch: "main",
    worktree_clean: true,
    engine_build_id: COMMIT,
  };
}

function evidence(id, extra) {
  return { id, sha256: HASH, source_commit: COMMIT, ...extra };
}

function artifact(role, name, sizeBytes) {
  return {
    role,
    name,
    sha256: HASH,
    size_bytes: sizeBytes,
    source_commit: COMMIT,
  };
}
