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
import {
  canonicalSha256,
  sealCanonicalReport,
} from "./canonical-release-evidence.mjs";

const COMMIT = "1".repeat(40);
const TREE = "2".repeat(40);
const HASH = "a".repeat(64);

test("materializes one complete hash-chained release attempt without overwriting output", async () => {
  await withAttempt(async ({ journal, root }) => {
    for (const event of completeEvents()) {
      await appendFinalSourceAttemptEvent({ journalPath: journal, ...event });
    }
    const producerReports = createProducerReports();
    const output = join(root, "final-source.json");
    const manifest = await materializeFinalSourceManifest({
      journalPath: journal,
      outputPath: output,
      ...producerReports,
    });
    assert.equal(validateFinalSourceRevalidation(manifest, {
      expectedSourceCommit: COMMIT,
      ...producerReports,
    }), true);
    assert.deepEqual(JSON.parse(await readFile(output, "utf8")), manifest);
    await assert.rejects(
      materializeFinalSourceManifest({
        journalPath: journal,
        outputPath: output,
        ...producerReports,
      }),
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
  assert.deepEqual(
    parseFinalSourceAttemptCliArguments([
      "materialize",
      "--journal", "attempt.jsonl",
      "--output", "final-source.json",
      "--discord-catalog-sync-report", "catalog-sync.json",
      "--production-observation-report", "observation.json",
    ]),
    {
      command: "materialize",
      values: {
        "--journal": "attempt.jsonl",
        "--output": "final-source.json",
        "--discord-catalog-sync-report": "catalog-sync.json",
        "--production-observation-report": "observation.json",
      },
    },
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
  const { discordCatalogSyncReport, productionObservationReport } =
    createProducerReports();
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
      application_id: "223456789012345678",
      image_digest: `sha256:${HASH}`,
      job_revision: "job-1",
      oracle_revision: "oracle-1",
      oracle_release_sha256: "d".repeat(64),
      oracle_settings_sha256: "e".repeat(64),
      traffic_percent: 100,
      command_catalog_sha256: HASH,
      command_catalog_prior_snapshot_sha256: "8".repeat(64),
      command_catalog_readback_sha256: "c".repeat(64),
      command_catalog_sync_report_sha256:
        discordCatalogSyncReport.report_sha256,
      catalog_synced: true,
      status: "active",
    }),
    event("rollback-snapshot", evidence("rollback", { status: "captured" })),
    event("observation", {
      report_schema_id: "clearra.production-observation.v1",
      source_commit: COMMIT,
      started_at: "2026-08-27T00:00:00.000Z",
      ended_at: "2026-08-27T00:20:00.000Z",
      duration_seconds: 1200,
      probe_spec_sha256: "6".repeat(64),
      status: "passed",
      report_sha256: productionObservationReport.report_sha256,
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

function createProducerReports() {
  const discordCatalogSyncReport = sealCanonicalReport({
    schema_id: "clearra.discord.command-catalog-sync.v1",
    source_commit: COMMIT,
    application_id: "223456789012345678",
    started_at: "2026-08-27T00:00:00.000Z",
    ended_at: "2026-08-27T00:00:01.000Z",
    status: "synchronized",
    changed: true,
    command_count: 2,
    expected_catalog_sha256: HASH,
    prior_catalog_sha256: "b".repeat(64),
    prior_snapshot_sha256: "8".repeat(64),
    current_before_sha256: "b".repeat(64),
    current_after_sha256: "c".repeat(64),
  });
  const identities = {
    cloud: {
      source_commit: COMMIT,
      engine_build_id: COMMIT,
      revision: "job-1",
      image_digest: `sha256:${HASH}`,
      traffic_percent: 100,
      cpu: "8",
      memory: "16Gi",
      concurrency: 1,
      min_instances: 0,
      max_instances: 4,
      startup_cpu_boost: true,
      contract_schema_version: "clearra.search.contract.v2",
      supply_semantics_id: "clearra.supply.projected-terminal-lookahead.v1",
      artifact_schema_version: "clearra.solution-data.v1",
      job_smoke_report_sha256: "7".repeat(64),
      stable_url: "https://clearra-current-job.example.run.app/",
      tagged_url: "https://v080---clearra-current-job.example.run.app/",
      status: "active",
    },
    discord: {
      source_commit: COMMIT,
      application_id: "223456789012345678",
      command_catalog_sha256: HASH,
      command_catalog_prior_snapshot_sha256: "8".repeat(64),
      command_catalog_readback_sha256: "c".repeat(64),
      command_catalog_sync_report_sha256:
        discordCatalogSyncReport.report_sha256,
      command_count: 2,
      command_names: ["1:help", "3:Get original GIF"],
      status: "active",
    },
    oracle: {
      source_commit: COMMIT,
      release_id: "oracle-1",
      release_tree_sha256: "d".repeat(64),
      settings_sha256: "e".repeat(64),
      candidate_revision: "job-1",
      candidate_url: "https://v080---clearra-current-job.example.run.app/",
      job_url: "https://v080---clearra-current-job.example.run.app/jobs",
      deployment_nonce: "9".repeat(64),
      gateway_pid: 1234,
      gateway_start_monotonic_usec: 123456789,
      boot_id: "12345678-1234-1234-1234-123456789abc",
      ready_record_observed: true,
      status: "active",
    },
    pages: {
      source_commit: COMMIT,
      engine_build_id: COMMIT,
      version: "0.8.0",
      deployment_id: "pages-1",
      artifact_sha256: HASH,
      base_path: "/Clearra",
      url: "https://daejunnom.github.io/Clearra/",
      status: "active",
    },
  };
  const productionObservationReport = sealCanonicalReport({
    schema_id: "clearra.production-observation.v1",
    source_commit: COMMIT,
    started_at: "2026-08-27T00:00:00.000Z",
    ended_at: "2026-08-27T00:20:00.000Z",
    duration_seconds: 1200,
    interval_seconds: 1200,
    probe_spec_sha256: "6".repeat(64),
    probe_adapters: ["cloud", "discord", "oracle", "pages"].map((surface) => ({
      surface,
      sha256: "5".repeat(64),
    })),
    status: "passed",
    surfaces: ["cloud", "discord", "oracle", "pages"].map((surface) => {
      const identity = identities[surface];
      const identitySha256 = canonicalSha256(identity);
      return {
        surface,
        identity,
        identity_sha256: identitySha256,
        observation_count: 2,
        observations: [0, 1].map((sequence) => ({
          sequence,
          observed_at: sequence === 0
            ? "2026-08-27T00:00:00.000Z"
            : "2026-08-27T00:20:00.000Z",
          identity_sha256: identitySha256,
          freshness: observationFreshness(surface, identity, sequence),
        })),
      };
    }),
  });
  return { discordCatalogSyncReport, productionObservationReport };
}

function observationFreshness(surface, identity, sequence) {
  const observedAt = sequence === 0
    ? "2026-08-27T00:00:00.000Z"
    : "2026-08-27T00:20:00.000Z";
  const probeId = (sequence + 1).toString(16).padStart(64, "0");
  if (surface === "oracle") {
    return {
      operation_marker: canonicalSha256({
        contract: "clearra.oracle.candidate-observation.v1",
        source_commit: identity.source_commit,
        candidate_revision: identity.candidate_revision,
        fresh_operation_at: observedAt,
        observed_at: observedAt,
      }),
      fresh_operation_at: observedAt,
      observed_at: observedAt,
    };
  }
  if (surface === "discord") {
    return { probe_id: probeId, readback_sha256: "c".repeat(64) };
  }
  if (surface === "cloud") {
    return {
      probe_id: probeId,
      service_readback_sha256: "1".repeat(64),
      revision_readback_sha256: "2".repeat(64),
      stable_health_sha256: "3".repeat(64),
      tagged_health_sha256: "4".repeat(64),
    };
  }
  return { probe_id: probeId, identity_readback_sha256: "8".repeat(64) };
}
