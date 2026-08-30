import assert from "node:assert/strict";
import { createHash } from "node:crypto";
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
  appendFinalSourceAttemptStage,
  initializeFinalSourceAttempt,
  materializeFinalSourceManifest,
  parseFinalSourceAttemptCliArguments,
} from "./final-source-attempt-journal.mjs";
import { canonicalJson, canonicalSha256, sealCanonicalReport } from "./canonical-release-evidence.mjs";
import {
  FINAL_SOURCE_STAGE_EVIDENCE_SCHEMA_ID,
  validateFinalSourceStageEvidence,
} from "./final-source-stage-evidence.mjs";

const COMMIT = "1".repeat(40);
const TREE = "2".repeat(40);
const HASH = "a".repeat(64);
const submittedStageReconstruction = Object.freeze({
  reconstructStages: async ({ submittedStages }) => ({
    stages: submittedStages.map(({ value }) => value),
  }),
});

test("atomically appends three producer stages and materializes the exact final source", async () => {
  await withAttempt(async (context) => {
    for (const stage of context.stages) {
      await appendFinalSourceAttemptStage({
        journalPath: context.journal,
        stageEvidencePath: stage.path,
        stageEvidenceFileSha256: stage.fileSha256,
      });
    }
    const output = join(context.root, "final-source.json");
    const manifest = await materializeFinalSourceManifest({
      journalPath: context.journal,
      outputPath: output,
      ...materializeInputs(context),
    }, submittedStageReconstruction);
    assert.equal(manifest.source.commit, COMMIT);
    assert.equal(manifest.immutable_release.status, "published");
    assert.equal(await readFile(output, "utf8"), `${canonicalJson(manifest)}\n`);
    await assert.rejects(
      materializeFinalSourceManifest({
        journalPath: context.journal,
        outputPath: output,
        ...materializeInputs(context),
      }, submittedStageReconstruction),
      (error) => error?.code === "EEXIST",
    );
  });
});

test("rejects out-of-order, duplicate, and wrong-raw-SHA stage append", async () => {
  await withAttempt(async (context) => {
    await assert.rejects(
      appendFinalSourceAttemptStage({
        journalPath: context.journal,
        stageEvidencePath: context.stages[1].path,
        stageEvidenceFileSha256: context.stages[1].fileSha256,
      }),
      /out of order/u,
    );
    await assert.rejects(
      appendFinalSourceAttemptStage({
        journalPath: context.journal,
        stageEvidencePath: context.stages[0].path,
        stageEvidenceFileSha256: "f".repeat(64),
      }),
      /raw file SHA-256 differs/u,
    );
    await appendFinalSourceAttemptStage({
      journalPath: context.journal,
      stageEvidencePath: context.stages[0].path,
      stageEvidenceFileSha256: context.stages[0].fileSha256,
    });
    await assert.rejects(
      appendFinalSourceAttemptStage({
        journalPath: context.journal,
        stageEvidencePath: context.stages[0].path,
        stageEvidenceFileSha256: context.stages[0].fileSha256,
      }),
      /out of order/u,
    );
  });
});

test("a failed atomic replacement leaves the journal byte-for-byte unchanged", async () => {
  await withAttempt(async (context) => {
    const before = await readFile(context.journal);
    await assert.rejects(
      appendFinalSourceAttemptStage({
        journalPath: context.journal,
        stageEvidencePath: context.stages[0].path,
        stageEvidenceFileSha256: context.stages[0].fileSha256,
      }, {
        replaceJournal: async () => {
          throw new Error("injected atomic writer failure");
        },
      }),
      /injected atomic writer failure/u,
    );
    assert.deepEqual(await readFile(context.journal), before);
  });
});

test("materialization rejects incomplete or substituted stage authorities", async () => {
  await withAttempt(async (context) => {
    await appendFinalSourceAttemptStage({
      journalPath: context.journal,
      stageEvidencePath: context.stages[0].path,
      stageEvidenceFileSha256: context.stages[0].fileSha256,
    });
    await assert.rejects(
      materializeFinalSourceManifest({
        journalPath: context.journal,
        outputPath: join(context.root, "incomplete.json"),
        ...materializeInputs(context),
      }, submittedStageReconstruction),
      /incomplete/u,
    );
    for (const stage of context.stages.slice(1)) {
      await appendFinalSourceAttemptStage({
        journalPath: context.journal,
        stageEvidencePath: stage.path,
        stageEvidenceFileSha256: stage.fileSha256,
      });
    }
    const substituted = structuredClone(context.stages[0].value);
    substituted.events[0].payload.tree = "3".repeat(40);
    const { report_sha256: _oldReportSha256, ...unsignedSubstituted } = substituted;
    const replacement = sealCanonicalReport(unsignedSubstituted);
    const replacementInput = await writeCanonical(
      context.root,
      "substituted-acceptance.json",
      replacement,
    );
    await assert.rejects(
      materializeFinalSourceManifest({
        journalPath: context.journal,
        outputPath: join(context.root, "substituted.json"),
        ...materializeInputs(context),
        acceptanceStageEvidencePath: replacementInput.path,
        acceptanceStageEvidenceFileSha256: replacementInput.fileSha256,
      }, submittedStageReconstruction),
      /differs from its exact stage evidence file/u,
    );
  });
});

test("materialization rejects a substituted reopened producer behind unchanged stage JSON", async () => {
  await withAttempt(async (context) => {
    for (const stage of context.stages) {
      await appendFinalSourceAttemptStage({
        journalPath: context.journal,
        stageEvidencePath: stage.path,
        stageEvidenceFileSha256: stage.fileSha256,
      });
    }
    const rebuilt = context.stages.map(({ value }) => value);
    const unsigned = structuredClone(rebuilt[1]);
    delete unsigned.report_sha256;
    unsigned.producer_inputs[0].file_sha256 = "f".repeat(64);
    rebuilt[1] = sealCanonicalReport(unsigned);
    await assert.rejects(
      materializeFinalSourceManifest({
        journalPath: context.journal,
        outputPath: join(context.root, "substituted-original.json"),
        ...materializeInputs(context),
      }, {
        reconstructStages: async () => ({ stages: rebuilt }),
      }),
      /deployment stage differs from its reopened original producers/u,
    );
  });
});

test("rejects stage and journal symbolic-link inputs", async (t) => {
  await withAttempt(async (context) => {
    const linkedStage = join(context.root, "linked-stage.json");
    const linkedJournal = join(context.root, "linked-journal.jsonl");
    try {
      await symlink(context.stages[0].path, linkedStage, "file");
      await symlink(context.journal, linkedJournal, "file");
    } catch (error) {
      if (["EPERM", "EACCES", "ENOTSUP"].includes(error?.code)) {
        t.skip(`symbolic links unavailable: ${error.code}`);
        return;
      }
      throw error;
    }
    await assert.rejects(
      appendFinalSourceAttemptStage({
        journalPath: context.journal,
        stageEvidencePath: linkedStage,
        stageEvidenceFileSha256: context.stages[0].fileSha256,
      }),
      /not a regular non-link file/u,
    );
    await assert.rejects(
      appendFinalSourceAttemptStage({
        journalPath: linkedJournal,
        stageEvidencePath: context.stages[0].path,
        stageEvidenceFileSha256: context.stages[0].fileSha256,
      }),
      /not a regular non-link file/u,
    );
  });
});

test("CLI exposes only stage-batch append and closed materialization inputs", () => {
  assert.deepEqual(parseFinalSourceAttemptCliArguments([
    "append-stage",
    "--journal", "attempt.jsonl",
    "--stage-evidence", "acceptance.json",
    "--stage-evidence-file-sha256", HASH,
  ]), {
    command: "append-stage",
    values: {
      "--journal": "attempt.jsonl",
      "--stage-evidence": "acceptance.json",
      "--stage-evidence-file-sha256": HASH,
    },
  });
  assert.throws(
    () => parseFinalSourceAttemptCliArguments([
      "append", "--journal", "x", "--kind", "source", "--payload", "x.json",
    ]),
    /unsupported final-source attempt command/u,
  );
  assert.throws(
    () => parseFinalSourceAttemptCliArguments([
      "append-stage", "--journal", "x", "--stage-evidence", "x.json",
    ]),
    /--stage-evidence-file-sha256 is required/u,
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
    const producers = createProducerReports();
    const sync = await writeCanonical(root, "discord-sync.json", producers.discordCatalogSyncReport);
    const observation = await writeCanonical(root, "observation.json", producers.productionObservationReport);
    const stages = [];
    for (const stage of createStageReports(sync, observation)) {
      validateFinalSourceStageEvidence(stage, {
        expectedStage: stage.stage,
        expectedSourceCommit: COMMIT,
      });
      stages.push(await writeCanonical(root, `${stage.stage}.json`, stage));
    }
    await body({ root, journal, stages, sync, observation });
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}

function materializeInputs(context) {
  return {
    acceptanceStageEvidencePath: context.stages[0].path,
    acceptanceStageEvidenceFileSha256: context.stages[0].fileSha256,
    deploymentStageEvidencePath: context.stages[1].path,
    deploymentStageEvidenceFileSha256: context.stages[1].fileSha256,
    publicationStageEvidencePath: context.stages[2].path,
    publicationStageEvidenceFileSha256: context.stages[2].fileSha256,
    discordCatalogSyncReportPath: context.sync.path,
    discordCatalogSyncReportFileSha256: context.sync.fileSha256,
    productionObservationReportPath: context.observation.path,
    productionObservationReportFileSha256: context.observation.fileSha256,
  };
}

async function writeCanonical(root, name, value) {
  const path = join(root, name);
  const raw = `${canonicalJson(value)}\n`;
  await writeFile(path, raw, { flag: "wx", mode: 0o600 });
  return Object.freeze({
    path,
    value,
    fileSha256: createHash("sha256").update(raw, "utf8").digest("hex"),
  });
}

function createStageReports(sync, observation) {
  const acceptanceEvents = [
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
    event("drift-audit", evidence("implementation-start", { phase: "implementation-start", status: "no-drift" })),
    event("drift-audit", evidence("release-freeze", { phase: "release-freeze", status: "no-drift" })),
    event("canonical-gate", evidence("release-acceptance", { status: "passed", readiness_open_count: 0 })),
    ...["desktop", "discord", "native", "wasm"].map((surface) =>
      event("surface-report", evidence(`${surface}-surface`, { surface, status: "passed" }))),
    event("release-artifact", artifact("linux-cli", "Clearra-CLI-v0.8.0-linux-x86_64", 1)),
    event("release-artifact", artifact("windows-cli", "Clearra-CLI-v0.8.0-windows-x86_64.exe", 2)),
    event("release-artifact", artifact("windows-gui", "Clearra-GUI-v0.8.0-windows-x86_64.exe", 3)),
  ];
  const deploymentEvents = [
    event("deployment-pages", {
      source_commit: COMMIT,
      deployment_id: "pages-1",
      artifact_sha256: HASH,
      status: "active",
    }),
    event("deployment-discord", discordDeployment(sync.value)),
    event("rollback-snapshot", evidence("clearra.rollback.snapshot-set.v1", { status: "captured" })),
    event("observation", {
      report_schema_id: observation.value.schema_id,
      source_commit: COMMIT,
      started_at: observation.value.started_at,
      ended_at: observation.value.ended_at,
      duration_seconds: observation.value.duration_seconds,
      probe_spec_sha256: observation.value.probe_spec_sha256,
      status: "passed",
      report_sha256: observation.value.report_sha256,
    }),
  ];
  const publicationEvents = [
    event("tag", { name: "v0.8.0", target_commit: COMMIT, annotated: true, remote_verified: true }),
    event("immutable-release", {
      tag: "v0.8.0",
      source_commit: COMMIT,
      workflow_run_id: "123",
      immutable: true,
      asset_count: 3,
      status: "published",
    }),
  ];
  return [
    stage("acceptance", [
      "canonical-acceptance", "implementation-start-audit", "legacy-alias-contract",
      "product-registry", "release-freeze-audit", "search-option-contract",
    ], acceptanceEvents),
    stage("deployment", [
      "cloud-candidate-smoke", "discord-canonical-catalog",
      "discord-catalog-sync", "discord-command-sync-authority",
      "discord-prior-snapshot", "oracle-candidate-observation",
      "oracle-rollback-capture", "pages-deployment", "production-observation",
      "production-probe-spec", "rollback-snapshot",
    ], deploymentEvents, new Map([
      ["discord-catalog-sync", [sync.value.report_sha256, sync.fileSha256]],
      ["production-observation", [observation.value.report_sha256, observation.fileSha256]],
    ])),
    stage("publication", [
      "release-publication", "release-publication-authority",
      "release-publication-receipt",
    ], publicationEvents),
  ];
}

function stage(stageName, roles, events, identities = new Map()) {
  return sealCanonicalReport({
    schema_id: FINAL_SOURCE_STAGE_EVIDENCE_SCHEMA_ID,
    stage: stageName,
    source_commit: COMMIT,
    producer_inputs: roles.map((role) => {
      const [evidenceSha256, fileSha256] = identities.get(role) ?? [HASH, HASH];
      return { role, schema_id: `clearra.test.${role}.v1`, evidence_sha256: evidenceSha256, file_sha256: fileSha256 };
    }),
    events,
    status: "passed",
  });
}

function event(kind, payload) {
  return { kind, payload };
}

function sourcePayload() {
  return { commit: COMMIT, tree: TREE, branch: "main", worktree_clean: true, engine_build_id: COMMIT };
}

function evidence(id, extra) {
  return { id, sha256: HASH, source_commit: COMMIT, ...extra };
}

function artifact(role, name, sizeBytes) {
  return { role, name, sha256: HASH, size_bytes: sizeBytes, source_commit: COMMIT };
}

function discordDeployment(sync) {
  return {
    source_commit: COMMIT,
    application_id: sync.application_id,
    image_digest: `sha256:${HASH}`,
    job_revision: "job-1",
    oracle_revision: "oracle-1",
    oracle_release_sha256: "d".repeat(64),
    oracle_settings_sha256: "e".repeat(64),
    traffic_percent: 100,
    command_catalog_sha256: sync.expected_catalog_sha256,
    command_catalog_prior_snapshot_sha256: sync.prior_snapshot_sha256,
    command_catalog_readback_sha256: sync.current_after_sha256,
    command_catalog_sync_report_sha256: sync.report_sha256,
    catalog_synced: true,
    status: "active",
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
    accepted_run_id: "456",
    accepted_run_attempt: "1",
    accepted_ctk3_manifest_sha256: "1".repeat(64),
    canonical_acceptance_evidence_sha256: "2".repeat(64),
    canonical_acceptance_evidence_file_sha256: "3".repeat(64),
    command_catalog_file_sha256: "4".repeat(64),
    command_sync_authority_sha256: "5".repeat(64),
    command_sync_authority_file_sha256: "6".repeat(64),
    prior_snapshot_sha256: "8".repeat(64),
    prior_catalog_sha256: "b".repeat(64),
    current_before_sha256: "b".repeat(64),
    current_after_sha256: "c".repeat(64),
  });
  const identities = productionIdentities(discordCatalogSyncReport);
  const productionObservationReport = sealCanonicalReport({
    schema_id: "clearra.production-observation.v1",
    source_commit: COMMIT,
    started_at: "2026-08-27T00:00:00.000Z",
    ended_at: "2026-08-27T00:20:00.000Z",
    duration_seconds: 1200,
    interval_seconds: 1200,
    probe_spec_sha256: "6".repeat(64),
    probe_adapters: ["cloud", "discord", "oracle", "pages"].map((surface) => ({ surface, sha256: "5".repeat(64) })),
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
          observed_at: sequence === 0 ? "2026-08-27T00:00:00.000Z" : "2026-08-27T00:20:00.000Z",
          identity_sha256: identitySha256,
          freshness: observationFreshness(surface, identity, sequence),
        })),
      };
    }),
  });
  return { discordCatalogSyncReport, productionObservationReport };
}

function productionIdentities(sync) {
  return {
    cloud: {
      source_commit: COMMIT, engine_build_id: COMMIT, revision: "job-1",
      image_digest: `sha256:${HASH}`, traffic_percent: 100, cpu: "8", memory: "16Gi",
      concurrency: 1, min_instances: 0, max_instances: 4, startup_cpu_boost: true,
      contract_schema_version: "clearra.search.contract.v2",
      supply_semantics_id: "clearra.supply.projected-terminal-lookahead.v1",
      artifact_schema_version: "clearra.solution-data.v1",
      job_smoke_report_sha256: "7".repeat(64),
      stable_url: "https://clearra-current-job.example.run.app/",
      tagged_url: "https://v080---clearra-current-job.example.run.app/", status: "active",
    },
    discord: {
      source_commit: COMMIT, application_id: sync.application_id,
      command_catalog_sha256: sync.expected_catalog_sha256,
      command_catalog_prior_snapshot_sha256: sync.prior_snapshot_sha256,
      command_catalog_readback_sha256: sync.current_after_sha256,
      command_catalog_sync_report_sha256: sync.report_sha256,
      accepted_run_id: sync.accepted_run_id,
      accepted_run_attempt: sync.accepted_run_attempt,
      accepted_ctk3_manifest_sha256: sync.accepted_ctk3_manifest_sha256,
      canonical_acceptance_evidence_sha256: sync.canonical_acceptance_evidence_sha256,
      canonical_acceptance_evidence_file_sha256: sync.canonical_acceptance_evidence_file_sha256,
      command_catalog_file_sha256: sync.command_catalog_file_sha256,
      command_sync_authority_sha256: sync.command_sync_authority_sha256,
      command_sync_authority_file_sha256: sync.command_sync_authority_file_sha256,
      command_count: 2, command_names: ["1:help", "3:Get original GIF"], status: "active",
    },
    oracle: {
      source_commit: COMMIT, release_id: "oracle-1", release_tree_sha256: "d".repeat(64),
      settings_sha256: "e".repeat(64), candidate_revision: "job-1",
      candidate_url: "https://v080---clearra-current-job.example.run.app/",
      job_url: "https://v080---clearra-current-job.example.run.app/jobs",
      deployment_nonce: "9".repeat(64), gateway_pid: 1234,
      gateway_start_monotonic_usec: 123456789,
      boot_id: "12345678-1234-1234-9234-123456789abc",
      ready_record_observed: true, status: "active",
    },
    pages: {
      source_commit: COMMIT, engine_build_id: COMMIT, version: "0.8.0",
      deployment_id: "pages-1", artifact_sha256: HASH, base_path: "/Clearra",
      url: "https://daejunnom.github.io/Clearra/", status: "active",
    },
  };
}

function observationFreshness(surface, identity, sequence) {
  const observedAt = sequence === 0 ? "2026-08-27T00:00:00.000Z" : "2026-08-27T00:20:00.000Z";
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
  if (surface === "discord") return { probe_id: probeId, readback_sha256: "c".repeat(64) };
  if (surface === "cloud") {
    return {
      probe_id: probeId,
      service_readback_sha256: "1".repeat(64),
      revision_readback_sha256: "2".repeat(64),
      stable_health_sha256: "3".repeat(64),
      tagged_health_sha256: "4".repeat(64),
    };
  }
  return {
    probe_id: probeId,
    deployment_readback_sha256: "7".repeat(64),
    identity_readback_sha256: "8".repeat(64),
  };
}
