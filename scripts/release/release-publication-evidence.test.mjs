import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import test from "node:test";

import {
  collectReleasePublicationEvidence,
  collectReleasePublicationEvidenceBundle,
  collectReleasePublicationReceipt,
  createGithubCliPublicationDependencies,
  expectedReleasePublicationReceiptArtifactName,
  expectedReleasePublicationEvidenceArtifactName,
  extractReleasePublicationReceiptFromArtifactZip,
  inspectReleasePublicationFinalizerAttempt,
  planReleasePublicationRecovery,
  recoverReleasePublication,
  resolveReleasePublicationFinalAuthority,
  validateReleasePublicationFinalAuthority,
  validateReleasePublicationEvidence,
  validateReleasePublicationReceipt,
} from "./release-publication-evidence.mjs";
import { canonicalJson, sealCanonicalReport } from "./canonical-release-evidence.mjs";

const COMMIT = "1".repeat(40);
const TAG_OBJECT = "2".repeat(40);
const HASH = "a".repeat(64);
const REPOSITORY = "daejunnom/Clearra";
const RUN_ID = "123";
const RUN_ATTEMPT = "1";
const FINALIZER_RUN_ID = "999";

test("local resolver uses closed gh api argv without reading a token environment variable", async () => {
  const calls = [];
  const dependencies = createGithubCliPublicationDependencies({
    repository: REPOSITORY,
    runGh: async (args, label) => {
      calls.push({ args, label });
      if (args.at(-1).endsWith("/zip")) return Buffer.from("zip");
      return Buffer.from('{"total_count":0,"artifacts":[]}');
    },
  });
  assert.deepEqual(
    await dependencies.apiGet("/actions/artifacts?per_page=100&page=1", "listing"),
    { total_count: 0, artifacts: [] },
  );
  assert.equal(
    (await dependencies.downloadArtifact(
      "https://api.github.com/repos/daejunnom/Clearra/actions/artifacts/42/zip",
      "archive",
    )).toString("utf8"),
    "zip",
  );
  assert.deepEqual(calls.map(({ args }) => args), [
    ["api", "--method", "GET", "-H", "X-GitHub-Api-Version: 2026-03-10", "/repos/daejunnom/Clearra/actions/artifacts?per_page=100&page=1"],
    ["api", "--method", "GET", "-H", "X-GitHub-Api-Version: 2026-03-10", "/repos/daejunnom/Clearra/actions/artifacts/42/zip"],
  ]);
  await assert.rejects(
    dependencies.downloadArtifact("https://example.test/token", "archive"),
    /closed artifact download path/u,
  );
});

test("failed tag attempts recover only an exact accepted partial draft before publication", async () => {
  const acceptance = acceptanceEvidence();
  const accepted = acceptance.final_source_fragments.release_artifacts;
  const partial = draftRelease([releaseAsset(accepted[0], 701)]);
  const complete = draftRelease(accepted.map((artifact, index) =>
    releaseAsset(artifact, 701 + index)));
  const published = {
    ...complete,
    draft: false,
    immutable: true,
    published_at: "2026-08-30T00:01:00.000Z",
  };
  let releaseRead = 0;
  const uploaded = [];
  let publishedDraft = false;
  const recoveryAuthority = authority(undefined, { runAttempt: "2" });
  const result = await recoverReleasePublication({
    ...recoveryAuthority,
    acceptanceEvidence: acceptance,
    productsDirectory: "products",
  }, {
    verifyAcceptanceEvidence: async () => undefined,
    uploadAssets: async (paths) => { uploaded.push(...paths); },
    publishDraft: async () => { publishedDraft = true; },
    apiGet: async (path) => {
      if (path === `/actions/runs/${RUN_ID}/attempts/1`) {
        return publicationRun({ attempt: 1, status: "completed", conclusion: "failure" });
      }
      if (path === `/actions/runs/${RUN_ID}/attempts/2`) {
        return publicationRun({ attempt: 2, status: "in_progress", conclusion: null });
      }
      if (path === "/git/ref/tags/v0.8.0") {
        return { ref: "refs/tags/v0.8.0", object: { type: "tag", sha: TAG_OBJECT } };
      }
      if (path === `/git/tags/${TAG_OBJECT}`) {
        return { sha: TAG_OBJECT, tag: "v0.8.0", object: { type: "commit", sha: COMMIT } };
      }
      if (path === "/releases/tags/v0.8.0") {
        return structuredClone([partial, complete, published][releaseRead++]);
      }
      throw new Error(`unexpected recovery API path: ${path}`);
    },
  });
  assert.equal(result.status, "published");
  assert.equal(publishedDraft, true);
  assert.equal(uploaded.length, 2);
  assert.ok(uploaded[0].endsWith(accepted[1].name));
  assert.ok(uploaded[1].endsWith(accepted[2].name));
  assert.equal(releaseRead, 3);
});

test("draft recovery rejects extra mismatched and non-uploaded assets before mutation", () => {
  const acceptance = acceptanceEvidence();
  const accepted = acceptance.final_source_fragments.release_artifacts;
  const valid = draftRelease([releaseAsset(accepted[0], 701)]);
  for (const mutate of [
    (draft) => draft.assets.push({ ...releaseAsset(accepted[1], 702), name: "extra.exe" }),
    (draft) => { draft.assets[0].digest = `sha256:${"f".repeat(64)}`; },
    (draft) => { draft.assets[0].state = "new"; },
  ]) {
    const changed = structuredClone(valid);
    mutate(changed);
    assert.throws(
      () => planReleasePublicationRecovery(changed, authority(), accepted),
      /draft asset differs/u,
    );
  }
});

test("captures an active tag receipt then finalizes only its completed successful immutable publication", async () => {
  const acceptance = acceptanceEvidence();
  const receipt = await collectReleasePublicationReceipt(authority(acceptance), {
    apiGet: fakeApi({ runStatus: "in_progress", runConclusion: null }),
  });
  assert.equal(validateReleasePublicationReceipt(receipt, {
    expectedRepository: REPOSITORY,
    expectedSourceCommit: COMMIT,
    expectedWorkflowRunId: RUN_ID,
    expectedWorkflowRunAttempt: RUN_ATTEMPT,
    acceptanceEvidence: acceptance,
  }), receipt);
  const receiptRaw = `${canonicalJson(receipt)}\n`;
  const receiptFileSha256 = sha256(receiptRaw);
  const receiptArchive = createStoredZip(receiptRaw);
  const artifactDigest = `sha256:${sha256(receiptArchive)}`;
  const evidence = await collectReleasePublicationEvidence({
    ...authority(),
  }, {
    apiGet: fakeApi({ runStatus: "completed", runConclusion: "success", artifactDigest }),
    downloadArtifact: async () => receiptArchive,
  });
  assert.equal(evidence.workflow_run.status, "completed");
  assert.equal(evidence.workflow_run.conclusion, "success");
  assert.equal(evidence.publication_receipt.file_sha256, receiptFileSha256);
  assert.equal(validateReleasePublicationEvidence(evidence, {
    expectedRepository: REPOSITORY,
    expectedSourceCommit: COMMIT,
    expectedWorkflowRunId: RUN_ID,
    expectedWorkflowRunAttempt: RUN_ATTEMPT,
    acceptanceEvidence: acceptance,
    receipt,
    receiptFileSha256,
  }), evidence);
});

test("receipt capture rejects completed runs, lightweight tags, and mutable releases", async () => {
  const acceptance = acceptanceEvidence();
  await assert.rejects(
    collectReleasePublicationReceipt(authority(acceptance), {
      apiGet: fakeApi({ runStatus: "completed", runConclusion: "success" }),
    }),
    /exact tag authority/u,
  );
  await assert.rejects(
    collectReleasePublicationReceipt(authority(acceptance), {
      apiGet: fakeApi({ tagType: "commit" }),
    }),
    /lightweight/u,
  );
  await assert.rejects(
    collectReleasePublicationReceipt(authority(acceptance), {
      apiGet: fakeApi({ immutable: false }),
    }),
    /immutable three-asset/u,
  );
});

test("retry receipt requires every exact prior attempt to be completed non-success", async () => {
  const acceptance = acceptanceEvidence();
  const retryAuthority = authority(acceptance, { runAttempt: "2" });
  const receipt = await collectReleasePublicationReceipt(retryAuthority, {
    apiGet: fakeApi({ runAttempt: "2", priorConclusions: ["failure"] }),
  });
  assert.equal(receipt.prior_attempts.count, 1);
  await assert.rejects(
    collectReleasePublicationReceipt(retryAuthority, {
      apiGet: fakeApi({ runAttempt: "2", priorConclusions: ["success"] }),
    }),
    /successful prior attempt/u,
  );
  await assert.rejects(
    collectReleasePublicationReceipt(retryAuthority, {
      apiGet: fakeApi({ runAttempt: "2", priorConclusions: [] }),
    }),
    /unexpected API path/u,
  );
});

test("finalizer rerun creates evidence only when every prior attempt is non-success", async () => {
  const options = authority(undefined, { finalizerRunAttempt: "2" });
  const recovery = await inspectReleasePublicationFinalizerAttempt(options, {
    apiGet: fakeApi({
      finalizerRunAttempt: "2",
      priorFinalizerConclusions: ["failure"],
    }),
  });
  assert.equal(recovery.skipBecausePriorSuccess, false);
  const alreadyComplete = await inspectReleasePublicationFinalizerAttempt(options, {
    apiGet: fakeApi({
      finalizerRunAttempt: "2",
      priorFinalizerConclusions: ["success"],
    }),
  });
  assert.equal(alreadyComplete.skipBecausePriorSuccess, true);
});

test("finalization rejects an unfinished run and a substituted or short-lived receipt artifact", async () => {
  const acceptance = acceptanceEvidence();
  const receipt = await collectReleasePublicationReceipt(authority(acceptance), {
    apiGet: fakeApi(),
  });
  const receiptArchive = createStoredZip(`${canonicalJson(receipt)}\n`);
  const artifactDigest = `sha256:${sha256(receiptArchive)}`;
  const options = {
    ...authority(),
  };
  await assert.rejects(
    collectReleasePublicationEvidence(options, {
      apiGet: fakeApi({ artifactDigest }),
      downloadArtifact: async () => receiptArchive,
    }),
    /exact tag authority/u,
  );
  await assert.rejects(
    collectReleasePublicationEvidence(options, {
      apiGet: fakeApi({ runStatus: "completed", runConclusion: "success", artifactHead: "3".repeat(40) }),
      downloadArtifact: async () => receiptArchive,
    }),
    /receipt artifact (?:listing )?differs/u,
  );
  await assert.rejects(
    collectReleasePublicationEvidence(options, {
      apiGet: fakeApi({ runStatus: "completed", runConclusion: "success", retentionDays: 1, artifactDigest }),
      downloadArtifact: async () => receiptArchive,
    }),
    /receipt artifact differs/u,
  );
});

test("closed evidence rejects extra fields, wrong receipt bytes, and asset mutations", async () => {
  const acceptance = acceptanceEvidence();
  const receipt = await collectReleasePublicationReceipt(authority(acceptance), {
    apiGet: fakeApi(),
  });
  const receiptFileSha256 = sha256(`${canonicalJson(receipt)}\n`);
  const receiptArchive = createStoredZip(`${canonicalJson(receipt)}\n`);
  const artifactDigest = `sha256:${sha256(receiptArchive)}`;
  const evidence = await collectReleasePublicationEvidence({
    ...authority(),
  }, {
    apiGet: fakeApi({ runStatus: "completed", runConclusion: "success", artifactDigest }),
    downloadArtifact: async () => receiptArchive,
  });
  assert.throws(
    () => validateReleasePublicationEvidence({ ...evidence, token: "forbidden" }),
    /closed schema/u,
  );
  assert.throws(
    () => extractReleasePublicationReceiptFromArtifactZip(
      Buffer.from(receiptArchive).fill(0, 0, 1),
      artifactDigest,
    ),
    /differs from the artifact API digest/u,
  );
  assert.throws(
    () => validateReleasePublicationEvidence(evidence, {
      acceptanceEvidence: acceptance,
      receipt,
      receiptFileSha256: "f".repeat(64),
    }),
    /captured receipt/u,
  );
  const mutated = structuredClone(evidence);
  mutated.assets[0].size_bytes += 1;
  assert.throws(
    () => validateReleasePublicationEvidence(mutated),
    /canonical content/u,
  );
});

test("receipt ZIP extraction rejects missing extra and link-shaped entries", async () => {
  const acceptance = acceptanceEvidence();
  const receipt = await collectReleasePublicationReceipt(authority(acceptance), {
    apiGet: fakeApi(),
  });
  const raw = `${canonicalJson(receipt)}\n`;
  for (const archive of [
    createStoredZip(raw, { name: "not-the-receipt.json" }),
    createStoredZip(raw, { mode: 0o120777 }),
  ]) {
    assert.throws(
      () => extractReleasePublicationReceiptFromArtifactZip(
        archive,
        `sha256:${sha256(archive)}`,
      ),
      /ZIP entry is invalid/u,
    );
  }
  const extraEntryClaim = createStoredZip(raw);
  extraEntryClaim.writeUInt16LE(2, extraEntryClaim.length - 14);
  extraEntryClaim.writeUInt16LE(2, extraEntryClaim.length - 12);
  assert.throws(
    () => extractReleasePublicationReceiptFromArtifactZip(
      extraEntryClaim,
      `sha256:${sha256(extraEntryClaim)}`,
    ),
    /central directory is not closed/u,
  );
});

test("tag retry recovery and workflow-run finalization remain event-bound and input-free", () => {
  const releaseWorkflow = readFileSync(
    new URL("../../.github/workflows/release-cli.yml", import.meta.url),
    "utf8",
  );
  const finalizerWorkflow = readFileSync(
    new URL("../../.github/workflows/finalize-release-publication.yml", import.meta.url),
    "utf8",
  );
  assertPublicationWorkflowContract(releaseWorkflow, finalizerWorkflow);
  assert.throws(
    () => assertPublicationWorkflowContract(
      releaseWorkflow.replace(
        "if [[ \"$GITHUB_RUN_ATTEMPT\" == '1' ]]; then",
        "if false; then",
      ),
      finalizerWorkflow,
    ),
    /first-attempt pre-existing release guard/u,
  );
  assert.throws(
    () => assertPublicationWorkflowContract(
      releaseWorkflow.replace(
        "node scripts/release/release-publication-evidence.mjs recover",
        "echo unsafe-draft-resume",
      ),
      finalizerWorkflow,
    ),
    /partial-draft recovery/u,
  );
  assert.throws(
    () => assertPublicationWorkflowContract(
      releaseWorkflow,
      finalizerWorkflow.replace("workflow_run:", "workflow_dispatch:"),
    ),
    /workflow_run completion trigger/u,
  );
});

test("global resolver admits exactly one completed successful finalizer artifact", async () => {
  const acceptance = acceptanceEvidence();
  const receipt = await collectReleasePublicationReceipt(authority(acceptance), {
    apiGet: fakeApi(),
  });
  const receiptArchive = createStoredZip(`${canonicalJson(receipt)}\n`);
  const originalArtifactDigest = `sha256:${sha256(receiptArchive)}`;
  const bundle = await collectReleasePublicationEvidenceBundle(authority(), {
    apiGet: fakeApi({
      runStatus: "completed",
      runConclusion: "success",
      artifactDigest: originalArtifactDigest,
    }),
    downloadArtifact: async () => receiptArchive,
  });
  const finalArchive = createStoredZipEntries([
    ["clearra-release-publication-evidence.v1.json", `${canonicalJson(bundle.report)}\n`],
    ["clearra-release-publication-receipt.v1.json", `${canonicalJson(bundle.receipt)}\n`],
  ]);
  const finalDigest = `sha256:${sha256(finalArchive)}`;
  const finalName = expectedReleasePublicationEvidenceArtifactName(authority());
  const resolved = await resolveReleasePublicationFinalAuthority(authority(), {
    apiGet: resolverApi({ finalName, finalDigest }),
    downloadArtifact: async () => finalArchive,
  });
  assert.equal(validateReleasePublicationFinalAuthority(resolved.authority, {
    expectedRepository: REPOSITORY,
    expectedSourceCommit: COMMIT,
    expectedWorkflowRunId: RUN_ID,
    expectedWorkflowRunAttempt: RUN_ATTEMPT,
    publicationEvidence: bundle.report,
    publicationEvidenceFileSha256: sha256(`${canonicalJson(bundle.report)}\n`),
    publicationReceipt: bundle.receipt,
    publicationReceiptFileSha256: sha256(`${canonicalJson(bundle.receipt)}\n`),
  }), resolved.authority);

  await assert.rejects(
    resolveReleasePublicationFinalAuthority(authority(), {
      apiGet: resolverApi({ finalName, finalDigest, wrongWorkflow: true }),
      downloadArtifact: async () => finalArchive,
    }),
    /finalizer attempt differs/u,
  );
  await assert.rejects(
    resolveReleasePublicationFinalAuthority(authority(), {
      apiGet: resolverApi({ finalName, finalDigest, duplicate: true }),
      downloadArtifact: async () => finalArchive,
    }),
    /exactly one successful authority/u,
  );
  const wrongSourceUnsigned = structuredClone(bundle.report);
  delete wrongSourceUnsigned.report_sha256;
  wrongSourceUnsigned.source_commit = "2".repeat(40);
  const wrongSourceArchive = createStoredZipEntries([
    [
      "clearra-release-publication-evidence.v1.json",
      `${canonicalJson(sealCanonicalReport(wrongSourceUnsigned))}\n`,
    ],
    [
      "clearra-release-publication-receipt.v1.json",
      `${canonicalJson(bundle.receipt)}\n`,
    ],
  ]);
  await assert.rejects(
    resolveReleasePublicationFinalAuthority(authority(), {
      apiGet: resolverApi({
        finalName,
        finalDigest: `sha256:${sha256(wrongSourceArchive)}`,
      }),
      downloadArtifact: async () => wrongSourceArchive,
    }),
    /source commit differs/u,
  );
});

function assertPublicationWorkflowContract(releaseWorkflow, finalizerWorkflow) {
  if (!releaseWorkflow.includes("if [[ \"$GITHUB_RUN_ATTEMPT\" == '1' ]]; then")) {
    throw new Error("publication workflow lost its first-attempt pre-existing release guard");
  }
  const draftRecovery = releaseWorkflow.indexOf(
    "node scripts/release/release-publication-evidence.mjs recover",
  );
  const immutableReadback = releaseWorkflow.indexOf("if [[ \"$immutable\" != 'true' ]]");
  const receiptCapture = releaseWorkflow.indexOf(
    "node scripts/release/release-publication-evidence.mjs capture",
  );
  const receiptUpload = releaseWorkflow.indexOf("- name: Upload canonical publication receipt");
  if (
    draftRecovery < 0 || immutableReadback <= draftRecovery ||
    receiptCapture <= immutableReadback ||
    receiptUpload <= receiptCapture ||
    !releaseWorkflow.includes("retention-days: 90") ||
    !releaseWorkflow.includes("--workflow-run-attempt \"$GITHUB_RUN_ATTEMPT\"")
  ) {
    throw new Error("publication partial-draft recovery or exact-attempt post-readback receipt is missing");
  }
  if (
    !finalizerWorkflow.includes("workflow_run:") ||
    !finalizerWorkflow.includes("types: [completed]") ||
    !finalizerWorkflow.includes("github.event.workflow_run.conclusion == 'success'") ||
    !finalizerWorkflow.includes("github.event.workflow_run.run_attempt") ||
    !finalizerWorkflow.includes("github.event.workflow_run.head_sha") ||
    !finalizerWorkflow.includes("--finalizer-workflow-run-id \"$GITHUB_RUN_ID\"") ||
    !finalizerWorkflow.includes("--finalizer-workflow-run-attempt \"$GITHUB_RUN_ATTEMPT\"") ||
    !finalizerWorkflow.includes("--output-directory release-publication-evidence") ||
    !finalizerWorkflow.includes("if: steps.finalization.outputs.upload_required == 'true'") ||
    !finalizerWorkflow.includes("name: ${{ steps.finalization.outputs.artifact_name }}") ||
    finalizerWorkflow.includes("workflow_dispatch:") ||
    finalizerWorkflow.includes("receipt-artifact-id") ||
    finalizerWorkflow.includes("receipt-artifact-digest") ||
    finalizerWorkflow.includes("--acceptance-evidence")
  ) {
    throw new Error("publication finalizer lost its workflow_run completion trigger authority");
  }
}

function authority(acceptance, {
  runAttempt = RUN_ATTEMPT,
  finalizerRunAttempt = "1",
} = {}) {
  return {
    repository: REPOSITORY,
    tag: "v0.8.0",
    sourceCommit: COMMIT,
    workflowRunId: RUN_ID,
    workflowRunAttempt: runAttempt,
    finalizerWorkflowRunId: FINALIZER_RUN_ID,
    finalizerWorkflowRunAttempt: finalizerRunAttempt,
    ...(acceptance === undefined ? {} : { acceptanceEvidence: acceptance }),
  };
}

function releaseAsset(artifact, id) {
  return {
    id,
    name: artifact.name,
    state: "uploaded",
    size: artifact.size_bytes,
    digest: `sha256:${artifact.sha256}`,
  };
}

function draftRelease(assets) {
  return {
    id: 800,
    tag_name: "v0.8.0",
    draft: true,
    prerelease: false,
    immutable: false,
    published_at: null,
    assets,
  };
}

function publicationRun({ attempt, status, conclusion }) {
  return {
    id: Number(RUN_ID),
    run_attempt: attempt,
    event: "push",
    head_branch: "v0.8.0",
    head_sha: COMMIT,
    path: ".github/workflows/release-cli.yml",
    status,
    conclusion,
  };
}

function acceptanceEvidence() {
  const artifacts = [
    ["linux-cli", "Clearra-CLI-v0.8.0-linux-x86_64", "3", 101],
    ["windows-cli", "Clearra-CLI-v0.8.0-windows-x86_64.exe", "4", 102],
    ["windows-gui", "Clearra-GUI-v0.8.0-windows-x86_64.exe", "5", 103],
  ].map(([role, name, digit, sizeBytes]) => ({
    role,
    name,
    sha256: digit.repeat(64),
    size_bytes: sizeBytes,
    source_commit: COMMIT,
  }));
  return sealCanonicalReport({
    schema_id: "clearra.canonical-acceptance-evidence.v1",
    repository: REPOSITORY,
    release_version: "0.8.0",
    pages_base_path: "/Clearra",
    source_commit: COMMIT,
    run_id: "456",
    run_attempt: "1",
    workflow_path: ".github/workflows/release-cli.yml",
    status: "passed",
    jobs: [
      "metadata",
      "ctk3",
      "linux-cli",
      "discord-bot",
      "release-acceptance-foundation",
      "release-acceptance-sanitizer",
      "release-acceptance-rust",
      "release-acceptance-pages",
      "release-acceptance",
      "windows-products",
    ]
      .map((name, index) => ({ name, job_id: String(index + 1), status: "passed" })),
    accepted_inputs: {
      ctk3_manifest_sha256: "6".repeat(64),
      pages_identity_sha256: "7".repeat(64),
      gate_index_sha256: "8".repeat(64),
    },
    final_source_fragments: {
      toolchains: {
        source_commit: COMMIT,
        manifest_sha256: HASH,
        rust: "rustc 1.90.0",
        node: "v24.0.0",
        wasm_bindgen: "wasm-bindgen 0.2.126",
      },
      canonical_gate: {
        id: "release-acceptance-run-456-attempt-1",
        sha256: HASH,
        source_commit: COMMIT,
        status: "passed",
        readiness_open_count: 0,
      },
      surface_reports: ["desktop", "discord", "native", "wasm"].map((surface) => ({
        id: `${surface}-run-456-attempt-1`,
        sha256: HASH,
        source_commit: COMMIT,
        surface,
        status: "passed",
      })),
      release_artifacts: artifacts,
    },
  });
}

function fakeApi({
  runStatus = "in_progress",
  runConclusion = null,
  tagType = "tag",
  immutable = true,
  artifactHead = COMMIT,
  retentionDays = 90,
  artifactDigest = `sha256:${"9".repeat(64)}`,
  runAttempt = RUN_ATTEMPT,
  priorConclusions = [],
  finalizerRunAttempt = "1",
  priorFinalizerConclusions = [],
} = {}) {
  const acceptance = acceptanceEvidence();
  const assets = acceptance.final_source_fragments.release_artifacts.map((artifact, index) => ({
    id: 700 + index,
    name: artifact.name,
    state: "uploaded",
    size: artifact.size_bytes,
    digest: `sha256:${artifact.sha256}`,
  }));
  const created = Date.parse("2026-08-30T00:00:00.000Z");
  const responses = new Map([
    [`/actions/runs/${FINALIZER_RUN_ID}/attempts/${finalizerRunAttempt}`, {
      id: Number(FINALIZER_RUN_ID),
      run_attempt: Number(finalizerRunAttempt),
      event: "workflow_run",
      path: ".github/workflows/finalize-release-publication.yml",
      status: "in_progress",
      conclusion: null,
    }],
    [`/actions/runs/${RUN_ID}/attempts/${runAttempt}`, {
      id: Number(RUN_ID),
      run_attempt: Number(runAttempt),
      event: "push",
      head_branch: "v0.8.0",
      head_sha: COMMIT,
      path: ".github/workflows/release-cli.yml",
      status: runStatus,
      conclusion: runConclusion,
    }],
    ["/git/ref/tags/v0.8.0", {
      ref: "refs/tags/v0.8.0",
      object: { type: tagType, sha: TAG_OBJECT },
    }],
    [`/git/tags/${TAG_OBJECT}`, {
      sha: TAG_OBJECT,
      tag: "v0.8.0",
      object: { type: "commit", sha: COMMIT },
    }],
    ["/releases/tags/v0.8.0", {
      id: 8001,
      tag_name: "v0.8.0",
      draft: false,
      prerelease: false,
      immutable,
      published_at: "2026-08-30T00:01:00.000Z",
      assets,
    }],
    ["/actions/artifacts/9001", {
      id: 9001,
      name: expectedReleasePublicationReceiptArtifactName({
        sourceCommit: COMMIT,
        workflowRunId: RUN_ID,
        workflowRunAttempt: runAttempt,
      }),
      digest: artifactDigest,
      archive_download_url: "https://api.github.com/repos/daejunnom/Clearra/actions/artifacts/9001/zip",
      expired: false,
      created_at: new Date(created).toISOString(),
      expires_at: new Date(created + retentionDays * 24 * 60 * 60 * 1000).toISOString(),
      workflow_run: {
        id: Number(RUN_ID),
        head_branch: "v0.8.0",
        head_sha: artifactHead,
      },
    }],
    [`/actions/runs/${RUN_ID}/artifacts?per_page=100`, {
      total_count: 1,
      artifacts: [{
        id: 9001,
        name: expectedReleasePublicationReceiptArtifactName({
          sourceCommit: COMMIT,
          workflowRunId: RUN_ID,
          workflowRunAttempt: runAttempt,
        }),
        digest: artifactDigest,
        expired: false,
        workflow_run: {
          id: Number(RUN_ID),
          head_branch: "v0.8.0",
          head_sha: artifactHead,
        },
      }],
    }],
  ]);
  priorConclusions.forEach((conclusion, index) => {
    const attempt = String(index + 1);
    responses.set(`/actions/runs/${RUN_ID}/attempts/${attempt}`, {
      id: Number(RUN_ID),
      run_attempt: Number(attempt),
      event: "push",
      head_branch: "v0.8.0",
      head_sha: COMMIT,
      path: ".github/workflows/release-cli.yml",
      status: "completed",
      conclusion,
    });
  });
  priorFinalizerConclusions.forEach((conclusion, index) => {
    const attempt = String(index + 1);
    responses.set(`/actions/runs/${FINALIZER_RUN_ID}/attempts/${attempt}`, {
      id: Number(FINALIZER_RUN_ID),
      run_attempt: Number(attempt),
      event: "workflow_run",
      path: ".github/workflows/finalize-release-publication.yml",
      status: "completed",
      conclusion,
    });
  });
  return async (path) => {
    if (!responses.has(path)) throw new Error(`unexpected API path: ${path}`);
    return structuredClone(responses.get(path));
  };
}

function resolverApi({ finalName, finalDigest, wrongWorkflow = false, duplicate = false }) {
  const created = Date.parse("2026-08-30T00:10:00.000Z");
  const candidate = (id) => ({
    id,
    name: finalName,
    digest: finalDigest,
    expired: false,
    workflow_run: { id: Number(FINALIZER_RUN_ID) },
  });
  const artifacts = duplicate ? [candidate(9100), candidate(9101)] : [candidate(9100)];
  const responses = new Map([
    ["/actions/artifacts?per_page=100&page=1", {
      total_count: artifacts.length,
      artifacts,
    }],
    [`/actions/runs/${FINALIZER_RUN_ID}/attempts/1`, {
      id: Number(FINALIZER_RUN_ID),
      run_attempt: 1,
      event: "workflow_run",
      path: wrongWorkflow
        ? ".github/workflows/not-the-finalizer.yml"
        : ".github/workflows/finalize-release-publication.yml",
      status: "completed",
      conclusion: "success",
    }],
  ]);
  for (const artifact of artifacts) {
    responses.set(`/actions/artifacts/${artifact.id}`, {
      ...artifact,
      archive_download_url:
        `https://api.github.com/repos/daejunnom/Clearra/actions/artifacts/${artifact.id}/zip`,
      created_at: new Date(created).toISOString(),
      expires_at: new Date(created + 90 * 24 * 60 * 60 * 1000).toISOString(),
    });
  }
  return async (path) => {
    if (!responses.has(path)) throw new Error(`unexpected resolver API path: ${path}`);
    return structuredClone(responses.get(path));
  };
}

function sha256(value) {
  return createHash("sha256").update(value).digest("hex");
}

function createStoredZip(raw, {
  name: entryName = "clearra-release-publication-receipt.v1.json",
  mode = 0o100600,
} = {}) {
  const name = Buffer.from(entryName, "utf8");
  const payload = Buffer.from(raw, "utf8");
  const checksum = crc32(payload);
  const local = Buffer.alloc(30);
  local.writeUInt32LE(0x04034b50, 0);
  local.writeUInt16LE(20, 4);
  local.writeUInt16LE(0x800, 6);
  local.writeUInt16LE(0, 8);
  local.writeUInt32LE(checksum, 14);
  local.writeUInt32LE(payload.length, 18);
  local.writeUInt32LE(payload.length, 22);
  local.writeUInt16LE(name.length, 26);
  const centralOffset = local.length + name.length + payload.length;
  const central = Buffer.alloc(46);
  central.writeUInt32LE(0x02014b50, 0);
  central.writeUInt16LE(0x0314, 4);
  central.writeUInt16LE(20, 6);
  central.writeUInt16LE(0x800, 8);
  central.writeUInt16LE(0, 10);
  central.writeUInt32LE(checksum, 16);
  central.writeUInt32LE(payload.length, 20);
  central.writeUInt32LE(payload.length, 24);
  central.writeUInt16LE(name.length, 28);
  central.writeUInt32LE((mode << 16) >>> 0, 38);
  central.writeUInt32LE(0, 42);
  const centralSize = central.length + name.length;
  const eocd = Buffer.alloc(22);
  eocd.writeUInt32LE(0x06054b50, 0);
  eocd.writeUInt16LE(1, 8);
  eocd.writeUInt16LE(1, 10);
  eocd.writeUInt32LE(centralSize, 12);
  eocd.writeUInt32LE(centralOffset, 16);
  return Buffer.concat([local, name, payload, central, name, eocd]);
}

function createStoredZipEntries(entries) {
  const localParts = [];
  const centralRecords = [];
  let localOffset = 0;
  for (const [entryName, raw] of entries) {
    const name = Buffer.from(entryName, "utf8");
    const payload = Buffer.from(raw, "utf8");
    const checksum = crc32(payload);
    const local = Buffer.alloc(30);
    local.writeUInt32LE(0x04034b50, 0);
    local.writeUInt16LE(20, 4);
    local.writeUInt16LE(0x800, 6);
    local.writeUInt16LE(0, 8);
    local.writeUInt32LE(checksum, 14);
    local.writeUInt32LE(payload.length, 18);
    local.writeUInt32LE(payload.length, 22);
    local.writeUInt16LE(name.length, 26);
    localParts.push(local, name, payload);
    const central = Buffer.alloc(46);
    central.writeUInt32LE(0x02014b50, 0);
    central.writeUInt16LE(0x0314, 4);
    central.writeUInt16LE(20, 6);
    central.writeUInt16LE(0x800, 8);
    central.writeUInt16LE(0, 10);
    central.writeUInt32LE(checksum, 16);
    central.writeUInt32LE(payload.length, 20);
    central.writeUInt32LE(payload.length, 24);
    central.writeUInt16LE(name.length, 28);
    central.writeUInt32LE((0o100600 << 16) >>> 0, 38);
    central.writeUInt32LE(localOffset, 42);
    centralRecords.push(central, name);
    localOffset += local.length + name.length + payload.length;
  }
  const centralBytes = Buffer.concat(centralRecords);
  const eocd = Buffer.alloc(22);
  eocd.writeUInt32LE(0x06054b50, 0);
  eocd.writeUInt16LE(entries.length, 8);
  eocd.writeUInt16LE(entries.length, 10);
  eocd.writeUInt32LE(centralBytes.length, 12);
  eocd.writeUInt32LE(localOffset, 16);
  return Buffer.concat([...localParts, centralBytes, eocd]);
}

function crc32(bytes) {
  let crc = 0xffffffff;
  for (const byte of bytes) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit += 1) {
      crc = (crc >>> 1) ^ ((crc & 1) === 1 ? 0xedb88320 : 0);
    }
  }
  return (crc ^ 0xffffffff) >>> 0;
}
