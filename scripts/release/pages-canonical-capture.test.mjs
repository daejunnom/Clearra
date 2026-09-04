import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import test from "node:test";

import { canonicalSha256 } from "./canonical-release-evidence.mjs";
import {
  acceptedPagesArtifactName,
  resolveAcceptedPagesArtifact,
  readCanonicalPublicResponse,
  validateCanonicalCaptureEvidence,
} from "./pages-canonical-capture.mjs";

const SHA = "1".repeat(40);

function artifact() {
  return {
    id: 77,
    name: acceptedPagesArtifactName(SHA, "55", "1"),
    expired: false,
    digest: `sha256:${"a".repeat(64)}`,
    created_at: "2026-08-01T00:00:00Z",
    expires_at: "2026-10-30T00:00:00Z",
    workflow_run: { id: 55, head_sha: SHA },
  };
}

function run() { return { id: 55, run_attempt: 1, head_sha: SHA, status: "completed", conclusion: "success", event: "workflow_dispatch", head_branch: "main", path: ".github/workflows/release-cli.yml" }; }

test("canonical artifact resolution is attempt-1, complete-pagination, unique, durable, and exact-name bound", () => {
  const acceptedRun = run();
  const pages = [{ total_count: 1, artifacts: [artifact()] }, { total_count: 1, artifacts: [] }];
  const result = resolveAcceptedPagesArtifact({ sourceCommit: SHA, acceptedRun, artifactPages: pages });
  assert.equal(result.accepted_artifact_id, "77");
  assert.equal(result.accepted_run_attempt, "1");
  assert.throws(() => resolveAcceptedPagesArtifact({ sourceCommit: SHA, acceptedRun: { ...acceptedRun, run_attempt: 2 }, artifactPages: pages }), /attempt-1/u);
  assert.throws(() => resolveAcceptedPagesArtifact({ sourceCommit: SHA, acceptedRun, artifactPages: [pages[0]] }), /pagination/u);
  assert.throws(() => resolveAcceptedPagesArtifact({ sourceCommit: SHA, acceptedRun, artifactPages: [{ total_count: 2, artifacts: [artifact(), { ...artifact(), id: 78 }] }, { total_count: 2, artifacts: [] }] }), /exactly one/u);
});

test("canonical evidence seals full identity bytes and descriptor set across two public reads", () => {
  const payload = Buffer.from("payload");
  const identity = { schema: "clearra.pages.identity.v2", sourceCommit: SHA, engineBuildId: SHA, contractSchemaVersion: "clearra.search.contract.v2", supplySemanticsId: "clearra.supply.projected-terminal-lookahead.v1", artifactSchemaVersion: "clearra.solution-data.v1", version: "0.8.0", acceptedRunId: "55", acceptedRunAttempt: "1", basePath: "/Clearra", files: [{ path: "index.html", size: payload.length, sha256: createHash("sha256").update(payload).digest("hex") }] };
  const identityBytes = Buffer.from(JSON.stringify(identity));
  const readback = { identity_sha256: canonicalSha256(identity), identity_bytes_sha256: createHash("sha256").update(identityBytes).digest("hex"), identity_bytes_size: identityBytes.length, file_set_sha256: canonicalSha256(identity.files), file_count: 1, total_bytes: payload.length };
  const authority = resolveAcceptedPagesArtifact({ sourceCommit: SHA, acceptedRun: run(), artifactPages: [{ total_count: 1, artifacts: [artifact()] }, { total_count: 1, artifacts: [] }] });
  const evidence = { ...authority, identity, ...readback, initial_public_readback: readback, preartifact_public_readback: readback };
  assert.equal(validateCanonicalCaptureEvidence(evidence, { sourceCommit: SHA }), evidence);
  assert.throws(() => validateCanonicalCaptureEvidence({ ...evidence, preartifact_public_readback: { ...readback, total_bytes: payload.length + 1 } }, { sourceCommit: SHA }), /changed or differ/u);
});

test("public reader aborts an oversized chunked response without Content-Length", async () => {
  let cancelled = false;
  const stream = new ReadableStream({
    pull(controller) { controller.enqueue(new Uint8Array(8)); },
    cancel() { cancelled = true; },
  });
  await assert.rejects(readCanonicalPublicResponse(new Response(stream, { status: 200 }), 7), /exceeds its bound/u);
  assert.equal(cancelled, true);
});
