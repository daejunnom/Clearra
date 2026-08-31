import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { createCloudBuildImageAuthority } from "./cloud-build-image-authority-v080.mjs";

const SOURCE = "1".repeat(40);
const PROJECT = "clearra-production";
const TAG = `asia-northeast1-docker.pkg.dev/${PROJECT}/clearra/clearra-current-job:source-${SOURCE}`;

function build(overrides = {}) {
  return {
    id: "12345678-1234-4234-8234-123456789abc",
    projectId: PROJECT,
    status: "SUCCESS",
    substitutions: {
      _IMAGE_NAME: "clearra-current-job",
      _REGION: "asia-northeast1",
      _REPOSITORY: "clearra",
      _SOURCE_COMMIT: SOURCE,
      _TAG: `source-${SOURCE}`,
    },
    images: [TAG],
    results: { images: [{ name: TAG, digest: `sha256:${"a".repeat(64)}` }] },
    sourceProvenance: {
      resolvedStorageSource: { bucket: "clearra-build-source", object: "source.tgz", generation: "42" },
    },
    ...overrides,
  };
}

async function fixture(value = build()) {
  const root = await mkdtemp(join(tmpdir(), "clearra-cloud-build-authority-"));
  const archive = join(root, "source.tar.gz");
  const readback = join(root, "build.json");
  await writeFile(archive, "exact source bytes", { mode: 0o600 });
  await writeFile(readback, `${JSON.stringify(value)}\n`, { mode: 0o600 });
  return { root, archive, readback };
}

test("binds one successful exact-source Cloud Build to its immutable image digest", async () => {
  const value = await fixture();
  try {
    const report = await createCloudBuildImageAuthority({
      sourceCommit: SOURCE,
      projectId: PROJECT,
      exactSourceArchivePath: value.archive,
      buildReadbackPath: value.readback,
    });
    assert.equal(report.build_status, "SUCCESS");
    assert.equal(
      report.image_digest,
      `asia-northeast1-docker.pkg.dev/${PROJECT}/clearra/clearra-current-job@sha256:${"a".repeat(64)}`,
    );
    assert.match(report.exact_source_archive_sha256, /^[0-9a-f]{64}$/u);
  } finally {
    await rm(value.root, { recursive: true, force: true });
  }
});

test("rejects failed, mutable, ambiguous, or source-mismatched Cloud Builds", async () => {
  for (const value of [
    build({ status: "FAILURE" }),
    build({ results: { images: [{ name: TAG, digest: "latest" }] } }),
    build({ results: { images: [
      { name: TAG, digest: `sha256:${"a".repeat(64)}` },
      { name: TAG, digest: `sha256:${"b".repeat(64)}` },
    ] } }),
    build({ substitutions: { ...build().substitutions, _SOURCE_COMMIT: "2".repeat(40) } }),
  ]) {
    const files = await fixture(value);
    try {
      await assert.rejects(
        createCloudBuildImageAuthority({
          sourceCommit: SOURCE,
          projectId: PROJECT,
          exactSourceArchivePath: files.archive,
          buildReadbackPath: files.readback,
        }),
      );
    } finally {
      await rm(files.root, { recursive: true, force: true });
    }
  }
});
