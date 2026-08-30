import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import test from "node:test";

import {
  createProductionProbeSpec,
  PRODUCTION_PROBE_AUTHORITY_SCHEMA_ID,
  validateProductionProbeAuthority,
} from "./materialize-production-probe-spec.mjs";
import { validateProductionProbeSpec } from "./observe-production-surfaces.mjs";

const COMMIT = "1".repeat(40);

test("materializes three tracked Node adapters and one explicit Oracle owner boundary", async () => {
  await withAuthorityFiles(async ({ authority, sharedAdapterPath }) => {
    const spec = await createProductionProbeSpec(authority, { sharedAdapterPath });
    assert.equal(validateProductionProbeSpec(spec, COMMIT), spec);
    assert.deepEqual(spec.probes.map(({ surface, runtime }) => [surface, runtime]), [
      ["cloud", "node"],
      ["discord", "node"],
      ["oracle", "powershell"],
      ["pages", "node"],
    ]);
    assert.equal(new Set([
      spec.probes[0].sha256,
      spec.probes[1].sha256,
      spec.probes[3].sha256,
    ]).size, 1);
    assert.equal(spec.probes[2].sha256, authority.oracle.adapter_sha256);
    assert.deepEqual(spec.probes[2].arguments.slice(0, 2), [
      "-Operation",
      "observe-candidate",
    ]);
    assert.doesNotMatch(
      JSON.stringify(spec),
      /CLEARRA_ORACLE_IDENTITY_FILE|DISCORD_TOKEN|api[_-]?key|password/iu,
    );
  });
});

test("probe authority rejects hash drift, secret fields, and mixed source identity", async () => {
  await withAuthorityFiles(async ({ authority, sharedAdapterPath }) => {
    const wrongHash = structuredClone(authority);
    wrongHash.oracle.adapter_sha256 = "f".repeat(64);
    await assert.rejects(
      createProductionProbeSpec(wrongHash, { sharedAdapterPath }),
      /differs from its approved SHA-256/u,
    );

    const secret = structuredClone(authority);
    secret.discord.api_token = "forbidden";
    assert.throws(
      () => validateProductionProbeAuthority(secret),
      /fields differ from the closed schema/u,
    );

    const mixed = structuredClone(authority);
    mixed.oracle.candidate_revision = "clearra-current-job-v080-2222222";
    assert.throws(
      () => validateProductionProbeAuthority(mixed),
      /differs from the accepted source/u,
    );

    await writeFile(authority.cloud.smoke_report_path, '{"drift":true}\n', "utf8");
    await assert.rejects(
      createProductionProbeSpec(authority, { sharedAdapterPath }),
      /Cloud candidate smoke evidence file SHA-256 changed/u,
    );
  });
});

async function withAuthorityFiles(body) {
  const root = await mkdtemp(join(tmpdir(), "clearra-probe-spec-"));
  try {
    const oracleAdapter = resolve(root, "invoke-release-deploy-v080.ps1");
    const sharedAdapterPath = resolve(root, "production-surface-probe-adapter.mjs");
    const catalogPath = resolve(root, "discord-command-catalog.json");
    const syncReportPath = resolve(root, "discord-command-catalog-sync.json");
    const smokeReportPath = resolve(root, "cloud-candidate-smoke.json");
    await writeFile(oracleAdapter, "param()\n", "utf8");
    await writeFile(sharedAdapterPath, "export {};\n", "utf8");
    await writeFile(catalogPath, "{}\n", "utf8");
    await writeFile(syncReportPath, "{}\n", "utf8");
    await writeFile(smokeReportPath, "{}\n", "utf8");
    const oracleBytes = await readFile(oracleAdapter);
    const emptyCanonicalFileSha256 = createHash("sha256")
      .update("{}\n", "utf8")
      .digest("hex");
    const authority = {
      schema_id: PRODUCTION_PROBE_AUTHORITY_SCHEMA_ID,
      source_commit: COMMIT,
      interval_seconds: 30,
      discord: {
        application_id: "223456789012345678",
        catalog_path: catalogPath,
        catalog_file_sha256: emptyCanonicalFileSha256,
        sync_report_path: syncReportPath,
        sync_report_file_sha256: emptyCanonicalFileSha256,
        timeout_seconds: 30,
      },
      cloud: {
        project_id: "clearra-prod1",
        region: "asia-northeast1",
        service_name: "clearra-current-job",
        revision: "clearra-current-job-v080-1111111",
        tag: "candidate-1111111",
        image_digest: `sha256:${"f".repeat(64)}`,
        smoke_report_path: smokeReportPath,
        smoke_report_file_sha256: emptyCanonicalFileSha256,
        timeout_seconds: 30,
      },
      oracle: {
        adapter_path: oracleAdapter,
        adapter_sha256: createHash("sha256").update(oracleBytes).digest("hex"),
        script_release_id: "v0.8.0-1111111",
        script_release_sha256: "d".repeat(64),
        candidate_url: "https://v080---clearra-current-job.example.run.app/",
        candidate_revision: "clearra-current-job-v080-1111111",
        oracle_release_id: "v0.8.0-1111111",
        oracle_release_sha256: "d".repeat(64),
        oracle_settings_sha256: "e".repeat(64),
        deployment_nonce: "9".repeat(64),
        verified_after: "2026-08-30T00:00:00.000Z",
        timeout_seconds: 60,
      },
      pages: {
        url: "https://daejunnom.github.io/Clearra/",
        deployment_id: "pages-deployment-123",
        artifact_sha256: "a".repeat(64),
        base_path: "/Clearra",
        accepted_run_id: "123456789",
        accepted_run_attempt: "2",
        timeout_seconds: 30,
      },
    };
    await body({ authority, sharedAdapterPath });
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}
