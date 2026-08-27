import assert from "node:assert/strict";
import test from "node:test";

import {
  parseFinalSourceCliArguments,
  validateFinalSourceRevalidation,
} from "./validate-final-source-revalidation.mjs";

const COMMIT = "1".repeat(40);
const HASH = "a".repeat(64);

function evidence(id, extra = {}) {
  return { id, sha256: HASH, source_commit: COMMIT, ...extra };
}

function validManifest() {
  return {
    schema_id: "clearra.final-source-revalidation.v1",
    release: "v0.8.0",
    source: {
      commit: COMMIT,
      tree: "2".repeat(40),
      branch: "main",
      worktree_clean: true,
      engine_build_id: COMMIT,
    },
    contracts: {
      source_commit: COMMIT,
      product_registry_schema_id: "clearra.product-capability-registry.v1",
      product_registry_sha256: HASH,
      search_option_contract_sha256: HASH,
      legacy_alias_contract_sha256: HASH,
      ctk3_contract_sha256: HASH,
      readiness_open_count: 0,
    },
    toolchains: {
      source_commit: COMMIT,
      manifest_sha256: HASH,
      rust: "rustc 1.89.0",
      node: "v22.0.0",
      wasm_bindgen: "0.2.126",
    },
    drift_audits: [
      evidence("implementation-start", { phase: "implementation-start", status: "no-drift" }),
      evidence("release-freeze", { phase: "release-freeze", status: "no-drift" }),
    ],
    canonical_gate: evidence("release-acceptance", {
      status: "passed",
      readiness_open_count: 0,
    }),
    surface_reports: [
      evidence("native-report", { surface: "native", status: "passed" }),
      evidence("wasm-report", { surface: "wasm", status: "passed" }),
      evidence("desktop-report", { surface: "desktop", status: "passed" }),
      evidence("discord-report", { surface: "discord", status: "passed" }),
    ],
    release_artifacts: [
      { role: "linux-cli", name: "Clearra-CLI-v0.8.0-linux-x86_64", sha256: HASH, size_bytes: 1, source_commit: COMMIT },
      { role: "windows-cli", name: "Clearra-CLI-v0.8.0-windows-x86_64.exe", sha256: HASH, size_bytes: 2, source_commit: COMMIT },
      { role: "windows-gui", name: "Clearra-GUI-v0.8.0-windows-x86_64.exe", sha256: HASH, size_bytes: 3, source_commit: COMMIT },
    ],
    deployment: {
      pages: {
        source_commit: COMMIT,
        deployment_id: "pages-1",
        artifact_sha256: HASH,
        status: "active",
      },
      discord: {
        source_commit: COMMIT,
        image_digest: `sha256:${HASH}`,
        job_revision: "job-1",
        oracle_revision: "oracle-1",
        traffic_percent: 100,
        command_catalog_sha256: HASH,
        catalog_synced: true,
        status: "active",
      },
      rollback_snapshot: evidence("rollback", { status: "captured" }),
    },
    observation: {
      source_commit: COMMIT,
      started_at: "2026-08-27T00:00:00.000Z",
      ended_at: "2026-08-27T00:20:00.000Z",
      duration_seconds: 1200,
      status: "passed",
      report_sha256: HASH,
    },
    tag: {
      name: "v0.8.0",
      target_commit: COMMIT,
      annotated: true,
      remote_verified: true,
    },
    immutable_release: {
      tag: "v0.8.0",
      source_commit: COMMIT,
      workflow_run_id: "123",
      immutable: true,
      asset_count: 3,
      status: "published",
    },
  };
}

test("accepts exactly one fully observed v0.8.0 source identity", () => {
  assert.equal(
    validateFinalSourceRevalidation(validManifest(), { expectedSourceCommit: COMMIT }),
    true,
  );
});

test("rejects mixed source identities and nonzero readiness", () => {
  const mixed = validManifest();
  mixed.surface_reports[2].source_commit = "3".repeat(40);
  assert.throws(() => validateFinalSourceRevalidation(mixed), /source commit differs/u);

  const open = validManifest();
  open.contracts.readiness_open_count = 1;
  assert.throws(() => validateFinalSourceRevalidation(open), /zero readiness/u);
});

test("requires exactly three named release artifacts and twenty observed minutes", () => {
  const missingArtifact = validManifest();
  missingArtifact.release_artifacts.pop();
  assert.throws(() => validateFinalSourceRevalidation(missingArtifact), /exactly three/u);

  const renamedArtifact = validManifest();
  renamedArtifact.release_artifacts[2].name = "Clearra-GUI-v0.8.0-windows-x86_64.zip";
  assert.throws(
    () => validateFinalSourceRevalidation(renamedArtifact),
    /canonical release asset/u,
  );

  const shortObservation = validManifest();
  shortObservation.observation.ended_at = "2026-08-27T00:19:59.000Z";
  shortObservation.observation.duration_seconds = 1199;
  assert.throws(() => validateFinalSourceRevalidation(shortObservation), /at least 1200/u);
});

test("rejects prior release authority and secret-shaped fields recursively", () => {
  const stale = validManifest();
  stale.canonical_gate.id = "v0.7.5-release-acceptance";
  assert.throws(() => validateFinalSourceRevalidation(stale), /v0\.7\.5 authority/u);

  const secret = validManifest();
  secret.deployment.discord.api_token = "forbidden";
  assert.throws(() => validateFinalSourceRevalidation(secret), /forbidden secret material/u);
});

test("requires active deployments, catalog sync, exact tag, and immutable release", () => {
  const inactive = validManifest();
  inactive.deployment.discord.traffic_percent = 99;
  assert.throws(() => validateFinalSourceRevalidation(inactive), /100 percent/u);

  const tag = validManifest();
  tag.tag.target_commit = "4".repeat(40);
  assert.throws(() => validateFinalSourceRevalidation(tag), /source commit differs/u);

  const mutable = validManifest();
  mutable.immutable_release.immutable = false;
  assert.throws(() => validateFinalSourceRevalidation(mutable), /not recorded as immutable/u);
});

test("validator CLI rejects unknown duplicate missing and malformed arguments", () => {
  assert.deepEqual(
    parseFinalSourceCliArguments([
      "--manifest",
      "report.json",
      "--expected-source-commit",
      COMMIT,
    ]),
    { manifestPath: "report.json", expectedSourceCommit: COMMIT },
  );
  assert.throws(() => parseFinalSourceCliArguments([]), /--manifest PATH is required/u);
  assert.throws(
    () => parseFinalSourceCliArguments(["--manifest", "one", "--manifest", "two"]),
    /duplicate/u,
  );
  assert.throws(
    () => parseFinalSourceCliArguments(["--manifest", "--expected-source-commit", COMMIT]),
    /requires one value/u,
  );
  assert.throws(
    () => parseFinalSourceCliArguments(["--manifest", "one", "--future"]),
    /unsupported/u,
  );
  assert.throws(
    () => parseFinalSourceCliArguments([
      "--manifest",
      "one",
      "--expected-source-commit",
      "abc",
    ]),
    /full lowercase SHA-1/u,
  );
});
