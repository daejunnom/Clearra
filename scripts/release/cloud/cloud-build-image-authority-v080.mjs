#!/usr/bin/env node

import { createHash } from "node:crypto";
import { lstat, open, readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const CLOUD_BUILD_IMAGE_AUTHORITY =
  "clearra.cloud-build-image-authority.v1";

const SOURCE_COMMIT = /^[0-9a-f]{40}$/u;
const PROJECT_ID = /^[a-z][a-z0-9-]{4,61}[a-z0-9]$/u;
const BUILD_ID = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/u;
const DIGEST = /^sha256:[0-9a-f]{64}$/u;
const POSITIVE_INTEGER = /^[1-9][0-9]{0,30}$/u;

export async function createCloudBuildImageAuthority({
  sourceCommit,
  projectId,
  exactSourceArchivePath,
  buildReadbackPath,
}) {
  requirePattern(sourceCommit, SOURCE_COMMIT, "source commit");
  requirePattern(projectId, PROJECT_ID, "GCP project ID");
  const archive = await readRegular(exactSourceArchivePath, "exact source archive");
  const readback = await readRegular(buildReadbackPath, "Cloud Build readback");
  let build;
  try {
    build = JSON.parse(readback.bytes.toString("utf8"));
  } catch {
    throw new Error("Cloud Build readback is not JSON");
  }
  if (build === null || typeof build !== "object" || Array.isArray(build)) {
    throw new Error("Cloud Build readback must be one build object");
  }
  if (build.status !== "SUCCESS") {
    throw new Error("Cloud Build did not finish with exact SUCCESS status");
  }
  requirePattern(build.id, BUILD_ID, "Cloud Build ID");
  if (build.projectId !== projectId) {
    throw new Error("Cloud Build project differs from deployment authority");
  }
  const expectedTag =
    `asia-northeast1-docker.pkg.dev/${projectId}/clearra/clearra-current-job:source-${sourceCommit}`;
  const substitutions = build.substitutions;
  const expectedSubstitutions = Object.freeze({
    _IMAGE_NAME: "clearra-current-job",
    _REGION: "asia-northeast1",
    _REPOSITORY: "clearra",
    _SOURCE_COMMIT: sourceCommit,
    _TAG: `source-${sourceCommit}`,
  });
  if (substitutions === null || typeof substitutions !== "object" || Array.isArray(substitutions)) {
    throw new Error("Cloud Build substitutions are unavailable");
  }
  for (const [name, expected] of Object.entries(expectedSubstitutions)) {
    if (substitutions[name] !== expected) {
      throw new Error(`Cloud Build substitution ${name} differs`);
    }
  }
  if (!Array.isArray(build.images) || build.images.length !== 1 || build.images[0] !== expectedTag) {
    throw new Error("Cloud Build requested image set is not exact");
  }
  const images = build?.results?.images;
  if (!Array.isArray(images) || images.length !== 1) {
    throw new Error("Cloud Build must return one immutable image result");
  }
  if (images[0]?.name !== expectedTag) {
    throw new Error("Cloud Build result image tag differs");
  }
  const digest = requirePattern(images[0]?.digest, DIGEST, "Cloud Build image digest");
  validateResolvedStorageSource(build?.sourceProvenance?.resolvedStorageSource);
  const report = {
    schema_id: CLOUD_BUILD_IMAGE_AUTHORITY,
    source_commit: sourceCommit,
    project_id: projectId,
    region: "asia-northeast1",
    build_id: build.id,
    build_status: "SUCCESS",
    exact_source_archive_sha256: archive.sha256,
    cloud_build_readback_sha256: readback.sha256,
    image_tag: expectedTag,
    image_digest: `${expectedTag.slice(0, expectedTag.lastIndexOf(":"))}@${digest}`,
  };
  return Object.freeze({ ...report, report_sha256: sha256(canonicalJson(report)) });
}

function validateResolvedStorageSource(value) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("Cloud Build resolved source upload is unavailable");
  }
  if (
    typeof value.bucket !== "string" || value.bucket.length < 3 ||
    typeof value.object !== "string" || value.object.length < 1 ||
    !POSITIVE_INTEGER.test(String(value.generation ?? ""))
  ) {
    throw new Error("Cloud Build resolved source upload is invalid");
  }
}

async function readRegular(path, label) {
  const target = resolve(String(path ?? ""));
  await assertSafeDirectoryChain(dirname(target));
  const metadata = await lstat(target);
  if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.size < 1) {
    throw new Error(`${label} must be a nonempty regular non-link file`);
  }
  const bytes = await readFile(target);
  return Object.freeze({
    bytes,
    sha256: createHash("sha256").update(bytes).digest("hex"),
  });
}

async function writeNew(path, report) {
  const target = resolve(String(path ?? ""));
  await assertSafeDirectoryChain(dirname(target));
  const handle = await open(target, "wx", 0o600);
  try {
    await handle.writeFile(`${canonicalJson(report)}\n`, "utf8");
    await handle.sync();
  } finally {
    await handle.close();
  }
}

async function assertSafeDirectoryChain(directory) {
  let current = resolve(directory);
  for (;;) {
    const metadata = await lstat(current);
    if (!metadata.isDirectory() || metadata.isSymbolicLink()) {
      throw new Error("Cloud Build authority path uses a non-directory or link");
    }
    const parent = dirname(current);
    if (parent === current) return;
    current = parent;
  }
}

function requirePattern(value, pattern, label) {
  if (typeof value !== "string" || !pattern.test(value)) {
    throw new Error(`${label} is invalid`);
  }
  return value;
}

function sha256(value) {
  return createHash("sha256").update(value, "utf8").digest("hex");
}

function canonicalJson(value) {
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  return `{${Object.keys(value).sort().map((key) =>
    `${JSON.stringify(key)}:${canonicalJson(value[key])}`).join(",")}}`;
}

function parse(args) {
  const allowed = new Set([
    "--source-commit", "--project", "--exact-source-archive",
    "--build-readback", "--output",
  ]);
  const values = {};
  for (let index = 0; index < args.length; index += 2) {
    const option = args[index];
    const value = args[index + 1];
    if (!allowed.has(option) || typeof value !== "string" || value.length === 0) {
      throw new Error("Cloud Build authority arguments are invalid");
    }
    if (Object.hasOwn(values, option)) throw new Error(`duplicate option: ${option}`);
    values[option] = value;
  }
  for (const option of allowed) if (!Object.hasOwn(values, option)) throw new Error(`${option} is required`);
  return values;
}

async function main() {
  const values = parse(process.argv.slice(2));
  const report = await createCloudBuildImageAuthority({
    sourceCommit: values["--source-commit"],
    projectId: values["--project"],
    exactSourceArchivePath: values["--exact-source-archive"],
    buildReadbackPath: values["--build-readback"],
  });
  await writeNew(values["--output"], report);
  process.stdout.write(`${CLOUD_BUILD_IMAGE_AUTHORITY} ${report.report_sha256}\n`);
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  try {
    await main();
  } catch (error) {
    process.stderr.write(
      `cloud_build_image_authority=failed reason=${error instanceof Error ? error.message : String(error)}\n`,
    );
    process.exitCode = 2;
  }
}
