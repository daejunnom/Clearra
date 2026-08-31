import assert from "node:assert/strict";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import test from "node:test";

import {
  createProductionProbeAuthorityV080,
  writeProductionProbeAuthorityV080,
} from "./create-production-probe-authority-v080.mjs";
import { canonicalJson } from "./canonical-release-evidence.mjs";

const SOURCE = "0123456789abcdef0123456789abcdef01234567";

function authority(root) {
  return {
    schema_id: "clearra.production-observation-probe-authority.v1",
    source_commit: SOURCE,
    interval_seconds: 1200,
    discord: {
      application_id: "223456789012345678",
      catalog_path: resolve(root, "catalog.json"),
      catalog_file_sha256: "1".repeat(64),
      sync_authority_path: resolve(root, "sync-authority.json"),
      sync_authority_file_sha256: "2".repeat(64),
      sync_report_path: resolve(root, "sync-report.json"),
      sync_report_file_sha256: "3".repeat(64),
      timeout_seconds: 60,
    },
    cloud: {
      project_id: "clearra-prod1",
      region: "asia-northeast1",
      service_name: "clearra-current-job",
      revision: "clearra-current-job-v080-0123456",
      tag: "candidate-0123456",
      image_digest: `sha256:${"4".repeat(64)}`,
      smoke_report_path: resolve(root, "smoke.json"),
      smoke_report_file_sha256: "5".repeat(64),
      timeout_seconds: 60,
    },
    oracle: {
      adapter_path: resolve(root, "invoke-release-deploy-v080.ps1"),
      adapter_sha256: "6".repeat(64),
      script_release_id: "v0.8.0-0123456",
      script_release_sha256: "7".repeat(64),
      candidate_url: "https://candidate.example.run.app",
      candidate_revision: "clearra-current-job-v080-0123456",
      oracle_release_id: "v0.8.0-0123456",
      oracle_release_sha256: "7".repeat(64),
      oracle_settings_sha256: "8".repeat(64),
      deployment_nonce: "9".repeat(64),
      verified_after: "2026-08-31T00:00:00.000Z",
      timeout_seconds: 60,
    },
    pages: {
      deployment_report_path: resolve(root, "pages.json"),
      deployment_report_file_sha256: "a".repeat(64),
      timeout_seconds: 60,
    },
  };
}

test("writes a canonical create-new 1200-second four-surface authority", async () => {
  const root = await mkdtemp(join(tmpdir(), "clearra-probe-authority-"));
  try {
    const output = join(root, "authority.json");
    const value = authority(root);
    await writeProductionProbeAuthorityV080(output, value);
    assert.equal(await readFile(output, "utf8"), `${canonicalJson(value)}\n`);
    await assert.rejects(
      writeProductionProbeAuthorityV080(output, value),
      /EEXIST/u,
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("derivation fails before reading evidence for an invalid exact source", async () => {
  await assert.rejects(
    createProductionProbeAuthorityV080({ sourceCommit: "main" }),
    /source commit/u,
  );
});

test("authority writer rejects shorter windows and secret-shaped fields", async () => {
  const root = await mkdtemp(join(tmpdir(), "clearra-probe-authority-"));
  try {
    const short = authority(root);
    short.interval_seconds = 60;
    await assert.rejects(
      writeProductionProbeAuthorityV080(join(root, "short.json"), short),
      /exactly 1200 seconds/u,
    );
    const secret = authority(root);
    secret.discord.token = "forbidden";
    await assert.rejects(
      writeProductionProbeAuthorityV080(join(root, "secret.json"), secret),
      /closed schema/u,
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
