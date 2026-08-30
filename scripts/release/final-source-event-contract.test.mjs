import assert from "node:assert/strict";
import test from "node:test";

import {
  createFinalSourceEventEvidence,
  FINAL_SOURCE_EVENT_EVIDENCE_SCHEMA_ID,
  stageForFinalSourceEventKind,
  validateFinalSourceEventEvidence,
  validateFinalSourceEventPayload,
} from "./final-source-event-contract.mjs";

const COMMIT = "1".repeat(40);
const HASH = "a".repeat(64);

test("every final-source kind is accepted only through its closed source-bound payload", () => {
  for (const [kind, payload] of validPayloads()) {
    assert.equal(validateFinalSourceEventPayload(kind, payload, COMMIT), payload);
    const evidence = createFinalSourceEventEvidence({
      sourceCommit: COMMIT,
      kind,
      payload,
      producerSchemaId: `clearra.test.${kind}.v1`,
      producerReportSha256: HASH,
    });
    assert.equal(evidence.schema_id, FINAL_SOURCE_EVENT_EVIDENCE_SCHEMA_ID);
    assert.equal(evidence.stage, stageForFinalSourceEventKind(kind));
    assert.equal(validateFinalSourceEventEvidence(evidence, {
      expectedSourceCommit: COMMIT,
      expectedKind: kind,
      expectedProducerSchemaId: `clearra.test.${kind}.v1`,
    }), evidence);
  }
});

test("event evidence rejects extra fields, source drift, and unapproved producer identity", () => {
  const evidence = createFinalSourceEventEvidence({
    sourceCommit: COMMIT,
    kind: "source",
    payload: validPayloads().get("source"),
    producerSchemaId: "clearra.test.source.v1",
    producerReportSha256: HASH,
  });
  assert.throws(
    () => validateFinalSourceEventEvidence({ ...evidence, extra: true }),
    /closed schema/u,
  );
  assert.throws(
    () => validateFinalSourceEventEvidence(evidence, {
      expectedSourceCommit: "2".repeat(40),
    }),
    /source differs/u,
  );
  assert.throws(
    () => validateFinalSourceEventEvidence(evidence, {
      expectedProducerSchemaId: "clearra.unapproved.v1",
    }),
    /producer schema is not approved/u,
  );
});

test("event payloads fail closed on secrets, prior authority, and kind/source mutation", () => {
  assert.throws(
    () => validateFinalSourceEventPayload("source", {
      ...validPayloads().get("source"),
      api_token: "forbidden",
    }, COMMIT),
    /secret material|closed schema/u,
  );
  assert.throws(
    () => validateFinalSourceEventPayload("rollback-snapshot", {
      id: "v0.7.5-release-authority",
      sha256: HASH,
      source_commit: COMMIT,
      status: "captured",
    }, COMMIT),
    /v0\.7\.5/u,
  );
  assert.throws(
    () => validateFinalSourceEventPayload("contracts", {
      ...validPayloads().get("contracts"),
      source_commit: "2".repeat(40),
    }, COMMIT),
    /differs from the final source/u,
  );
  assert.throws(
    () => stageForFinalSourceEventKind("future-event"),
    /unsupported final-source event kind/u,
  );
});

function validPayloads() {
  return new Map([
    ["source", {
      commit: COMMIT,
      tree: "2".repeat(40),
      branch: "main",
      worktree_clean: true,
      engine_build_id: COMMIT,
    }],
    ["contracts", {
      source_commit: COMMIT,
      product_registry_schema_id: "clearra.product-capability-registry.v1",
      product_registry_sha256: HASH,
      search_option_contract_sha256: HASH,
      legacy_alias_contract_sha256: HASH,
      ctk3_contract_sha256: HASH,
      readiness_open_count: 0,
    }],
    ["toolchains", {
      source_commit: COMMIT,
      manifest_sha256: HASH,
      rust: "rustc 1.90.0",
      node: "v24.0.0",
      wasm_bindgen: "wasm-bindgen 0.2.126",
    }],
    ["drift-audit", evidence("drift", { phase: "release-freeze", status: "no-drift" })],
    ["canonical-gate", evidence("gate", { status: "passed", readiness_open_count: 0 })],
    ["surface-report", evidence("native", { surface: "native", status: "passed" })],
    ["release-artifact", {
      role: "linux-cli",
      name: "Clearra-CLI-v0.8.0-linux-x86_64",
      sha256: HASH,
      size_bytes: 1,
      source_commit: COMMIT,
    }],
    ["deployment-pages", {
      source_commit: COMMIT,
      deployment_id: "pages-1",
      artifact_sha256: HASH,
      status: "active",
    }],
    ["deployment-discord", {
      source_commit: COMMIT,
      application_id: "223456789012345678",
      image_digest: `sha256:${HASH}`,
      job_revision: "job-1",
      oracle_revision: "oracle-1",
      oracle_release_sha256: HASH,
      oracle_settings_sha256: HASH,
      traffic_percent: 100,
      command_catalog_sha256: HASH,
      command_catalog_prior_snapshot_sha256: HASH,
      command_catalog_readback_sha256: HASH,
      command_catalog_sync_report_sha256: HASH,
      catalog_synced: true,
      status: "active",
    }],
    ["rollback-snapshot", evidence("rollback-set", { status: "captured" })],
    ["observation", {
      report_schema_id: "clearra.production-observation.v1",
      source_commit: COMMIT,
      started_at: "2026-08-30T00:00:00.000Z",
      ended_at: "2026-08-30T00:20:00.000Z",
      duration_seconds: 1200,
      probe_spec_sha256: HASH,
      status: "passed",
      report_sha256: HASH,
    }],
    ["tag", { name: "v0.8.0", target_commit: COMMIT, annotated: true, remote_verified: true }],
    ["immutable-release", {
      tag: "v0.8.0",
      source_commit: COMMIT,
      workflow_run_id: "123",
      immutable: true,
      asset_count: 3,
      status: "published",
    }],
  ]);
}

function evidence(id, extra) {
  return { id, sha256: HASH, source_commit: COMMIT, ...extra };
}
