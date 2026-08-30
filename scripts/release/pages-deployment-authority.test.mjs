import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import {
  producePagesDeploymentAuthority,
  validatePagesDeploymentAuthorityReport,
  writePagesDeploymentAuthorityReport,
} from "./pages-deployment-authority.mjs";

const SOURCE = "1".repeat(40);
const AUTHORITY = "2".repeat(40);
const ARTIFACT_SHA256 = "3".repeat(64);

function identity(sourceCommit = SOURCE, { legacyRestore = false } = {}) {
  const base = {
    schema: "clearra.pages.identity.v2",
    sourceCommit,
    engineBuildId: sourceCommit,
    contractSchemaVersion: "clearra.search.contract.v2",
    supplySemanticsId: "clearra.supply.projected-terminal-lookahead.v1",
    artifactSchemaVersion: "clearra.solution-data.v1",
    version: legacyRestore ? "0.7.5" : "0.8.0",
  };
  if (legacyRestore) return base;
  return {
    ...base,
    acceptedRunId: "11111",
    acceptedRunAttempt: "2",
    basePath: "/Clearra",
    files: [
      { path: "index.html", size: 128, sha256: "4".repeat(64) },
      { path: "wasm/clearra_wasm_bg.wasm", size: 256, sha256: "5".repeat(64) },
    ],
  };
}

function fixture({ mode = "forward" } = {}) {
  const workflowSourceCommit = mode === "forward" ? SOURCE : AUTHORITY;
  const workflowPath = mode === "forward"
    ? ".github/workflows/pages.yml"
    : ".github/workflows/pages-rollback.yml";
  const input = {
    mode,
    repository: "daejunnom/Clearra",
    sourceCommit: SOURCE,
    workflowSourceCommit,
    workflowRunId: "22222",
    workflowRunAttempt: "3",
    artifactId: "33333",
    artifactName: "github-pages",
    pageUrl: "https://daejunnom.github.io/Clearra/",
    basePath: "/Clearra",
    acceptedRunId: mode === "forward" ? "11111" : undefined,
    acceptedRunAttempt: mode === "forward" ? "2" : undefined,
  };
  const responses = {
    "/actions/runs/22222": {
      id: 22222,
      run_attempt: 3,
      event: "workflow_dispatch",
      head_branch: "main",
      head_sha: workflowSourceCommit,
      path: workflowPath,
      status: "in_progress",
      conclusion: null,
    },
    "/actions/artifacts/33333": {
      id: 33333,
      name: "github-pages",
      expired: false,
      digest: `sha256:${ARTIFACT_SHA256}`,
      created_at: "2026-08-30T00:00:00Z",
      expires_at: "2026-08-31T00:00:00Z",
      workflow_run: {
        id: 22222,
        head_branch: "main",
        head_sha: workflowSourceCommit,
      },
    },
    "/pages": { html_url: "https://daejunnom.github.io/Clearra" },
    [`/pages/deployments/${workflowSourceCommit}`]: { status: "succeed" },
  };
  return {
    input,
    responses,
    dependencies: {
      async getGithubJson(path) {
        assert.ok(Object.hasOwn(responses, path), `unexpected GitHub read: ${path}`);
        return structuredClone(responses[path]);
      },
      async fetchPublicJson(url) {
        assert.match(url, /clearra-build-identity\.json\?authority=22222-3-1$/u);
        return identity(SOURCE, { legacyRestore: mode === "restore" });
      },
      async sleep() {
        assert.fail("happy-path producer must not sleep");
      },
      attempts: 2,
    },
  };
}

test("seals forward artifact, run-attempt, deployment status, and live identity API readbacks", async () => {
  const { input, dependencies } = fixture();
  const report = await producePagesDeploymentAuthority(input, dependencies);
  assert.equal(report.deployment_id, SOURCE);
  assert.equal(report.artifact_sha256, ARTIFACT_SHA256);
  assert.equal(report.accepted_run_id, "11111");
  assert.equal(report.deployment_status, "succeed");
  assert.equal(validatePagesDeploymentAuthorityReport(report, {
    expectedSourceCommit: SOURCE,
  }), report);
  for (const field of [
    "artifact_api_readback_sha256",
    "workflow_run_api_readback_sha256",
    "deployment_api_readback_sha256",
    "pages_configuration_api_readback_sha256",
    "live_identity_sha256",
    "report_sha256",
  ]) {
    assert.match(report[field], /^[0-9a-f]{64}$/u);
  }
  assert.doesNotMatch(JSON.stringify(report), /token|password|credential/iu);
});

test("restore authority derives accepted identity and queries the deploy action workflow SHA", async () => {
  const { input, dependencies } = fixture({ mode: "restore" });
  const report = await producePagesDeploymentAuthority(input, dependencies);
  assert.equal(report.source_commit, SOURCE);
  assert.equal(report.workflow_source_commit, AUTHORITY);
  assert.equal(report.deployment_id, AUTHORITY);
  assert.equal(report.accepted_run_id, null);
  assert.equal(report.workflow_path, ".github/workflows/pages-rollback.yml");
  validatePagesDeploymentAuthorityReport(report);
});

test("rejects artifact digest, run attempt, deployment status, and public identity drift", async () => {
  {
    const { input, dependencies, responses } = fixture();
    responses["/actions/artifacts/33333"].digest = "legacy-digest";
    await assert.rejects(
      producePagesDeploymentAuthority(input, dependencies),
      /artifact digest has an invalid format/u,
    );
  }
  {
    const { input, dependencies, responses } = fixture();
    responses["/actions/runs/22222"].run_attempt = 4;
    await assert.rejects(
      producePagesDeploymentAuthority(input, dependencies),
      /active exact-main attempt/u,
    );
  }
  {
    const { input, dependencies, responses } = fixture();
    responses[`/pages/deployments/${SOURCE}`].status = "queued";
    dependencies.attempts = 1;
    await assert.rejects(
      producePagesDeploymentAuthority(input, dependencies),
      /has not converged to succeed/u,
    );
  }
  {
    const { input, dependencies } = fixture();
    dependencies.fetchPublicJson = async () => identity(AUTHORITY);
    dependencies.attempts = 1;
    await assert.rejects(
      producePagesDeploymentAuthority(input, dependencies),
      /differs from the deployed source contract/u,
    );
  }
});

test("writes one canonical exclusive report file and rejects tampering", async () => {
  const root = await mkdtemp(join(tmpdir(), "clearra-pages-authority-"));
  try {
    const { input, dependencies } = fixture();
    const report = await producePagesDeploymentAuthority(input, dependencies);
    const path = join(root, "pages-authority.json");
    const fileSha256 = await writePagesDeploymentAuthorityReport(path, report);
    const raw = await readFile(path, "utf8");
    assert.equal(
      fileSha256,
      createHash("sha256").update(raw, "utf8").digest("hex"),
    );
    assert.equal(raw.endsWith("\n"), true);
    await assert.rejects(
      writePagesDeploymentAuthorityReport(path, report),
      /EEXIST/u,
    );
    const tampered = { ...report, artifact_sha256: "f".repeat(64) };
    assert.throws(
      () => validatePagesDeploymentAuthorityReport(tampered),
      /canonical content/u,
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
