import { createHash } from "node:crypto";
import {
  lstat,
  readFile,
  readdir,
  writeFile,
} from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const ACCEPTED_CTK3_MANIFEST = "clearra-accepted-ctk3.v2.json";
export const ACCEPTED_CTK3_CONTRACT = "clearra.accepted-ctk3-dist.v2";

const REQUIRED_FILES = Object.freeze([
  "decodeWorker.js",
  "index.cjs",
  "index.d.ts",
  "index.js",
]);
const SOURCE_COMMIT_PATTERN = /^[0-9a-f]{40}$/u;
const RUN_AUTHORITY_PATTERN = /^[1-9][0-9]{0,19}$/u;

export async function sealAcceptedCtk3Dist(
  distPath,
  sourceCommit,
  runId,
  runAttempt,
) {
  requireSourceCommit(sourceCommit, "source commit");
  requireRunAuthority(runId, "run ID");
  requireRunAuthority(runAttempt, "run attempt");
  const root = resolve(distPath);
  await requireDirectory(root);
  const manifestPath = resolve(root, ACCEPTED_CTK3_MANIFEST);
  if (await pathExists(manifestPath)) {
    throw new Error(`accepted CTK3 manifest already exists: ${manifestPath}`);
  }

  const files = await collectPayloadFiles(root);
  requireCoreFiles(files);
  const manifest = {
    contract: ACCEPTED_CTK3_CONTRACT,
    source_commit: sourceCommit,
    run_id: runId,
    run_attempt: runAttempt,
    files,
  };
  await writeFile(
    manifestPath,
    `${JSON.stringify(manifest, null, 2)}\n`,
    { encoding: "utf8", flag: "wx" },
  );
  return manifest;
}

export async function verifyAcceptedCtk3Dist(
  distPath,
  expectedSourceCommit,
  expectedRunId,
  expectedRunAttempt,
) {
  requireSourceCommit(expectedSourceCommit, "expected source commit");
  requireRunAuthority(expectedRunId, "expected run ID");
  requireRunAuthority(expectedRunAttempt, "expected run attempt");
  const root = resolve(distPath);
  await requireDirectory(root);
  const manifestPath = resolve(root, ACCEPTED_CTK3_MANIFEST);
  const manifestStats = await lstat(manifestPath).catch((error) => {
    if (error?.code === "ENOENT") {
      throw new Error(`accepted CTK3 manifest is missing: ${manifestPath}`);
    }
    throw error;
  });
  if (!manifestStats.isFile() || manifestStats.isSymbolicLink()) {
    throw new Error("accepted CTK3 manifest must be a regular file");
  }

  let manifest;
  try {
    manifest = JSON.parse(await readFile(manifestPath, "utf8"));
  } catch (error) {
    throw new Error(`accepted CTK3 manifest is not valid JSON: ${error.message}`);
  }
  requireExactKeys(
    manifest,
    ["contract", "files", "run_attempt", "run_id", "source_commit"],
    "accepted CTK3 manifest",
  );
  if (manifest.contract !== ACCEPTED_CTK3_CONTRACT) {
    throw new Error(`accepted CTK3 contract mismatch: ${manifest.contract}`);
  }
  requireSourceCommit(manifest.source_commit, "manifest source commit");
  if (manifest.source_commit !== expectedSourceCommit) {
    throw new Error(
      `accepted CTK3 source commit mismatch: expected ${expectedSourceCommit}, received ${manifest.source_commit}`,
    );
  }
  requireRunAuthority(manifest.run_id, "manifest run ID");
  if (manifest.run_id !== expectedRunId) {
    throw new Error(
      `accepted CTK3 run ID mismatch: expected ${expectedRunId}, received ${manifest.run_id}`,
    );
  }
  requireRunAuthority(manifest.run_attempt, "manifest run attempt");
  if (manifest.run_attempt !== expectedRunAttempt) {
    throw new Error(
      `accepted CTK3 run attempt mismatch: expected ${expectedRunAttempt}, received ${manifest.run_attempt}`,
    );
  }
  if (!Array.isArray(manifest.files) || manifest.files.length === 0) {
    throw new Error("accepted CTK3 manifest files must be a non-empty array");
  }
  let previousPath = "";
  for (const [index, entry] of manifest.files.entries()) {
    requireExactKeys(
      entry,
      ["path", "sha256", "size"],
      `accepted CTK3 manifest file ${index}`,
    );
    if (
      typeof entry.path !== "string" ||
      entry.path.length === 0 ||
      entry.path.includes("\\") ||
      entry.path.startsWith("/") ||
      entry.path.split("/").includes("..")
    ) {
      throw new Error(`accepted CTK3 manifest file ${index} has an unsafe path`);
    }
    if (
      previousPath.length > 0 &&
      previousPath.localeCompare(entry.path, "en") >= 0
    ) {
      throw new Error("accepted CTK3 manifest file paths must be unique and sorted");
    }
    previousPath = entry.path;
    if (!Number.isSafeInteger(entry.size) || entry.size < 0) {
      throw new Error(`accepted CTK3 manifest file ${entry.path} has an invalid size`);
    }
    if (typeof entry.sha256 !== "string" || !/^[0-9a-f]{64}$/u.test(entry.sha256)) {
      throw new Error(`accepted CTK3 manifest file ${entry.path} has an invalid SHA-256`);
    }
  }
  requireCoreFiles(manifest.files);

  const actualFiles = await collectPayloadFiles(root);
  if (JSON.stringify(actualFiles) !== JSON.stringify(manifest.files)) {
    throw new Error(
      "accepted CTK3 distribution does not match its sealed file set and hashes",
    );
  }
  return manifest;
}

async function collectPayloadFiles(root) {
  const files = [];
  await walk(root, "", files);
  files.sort((left, right) => left.path.localeCompare(right.path, "en"));
  return files;
}

async function walk(root, relativeDirectory, files) {
  const absoluteDirectory = relativeDirectory
    ? resolve(root, ...relativeDirectory.split("/"))
    : root;
  const entries = await readdir(absoluteDirectory, { withFileTypes: true });
  entries.sort((left, right) => left.name.localeCompare(right.name, "en"));
  for (const entry of entries) {
    const relativePath = relativeDirectory
      ? `${relativeDirectory}/${entry.name}`
      : entry.name;
    if (relativePath === ACCEPTED_CTK3_MANIFEST) continue;
    if (/^clearra-accepted-ctk3\.v[0-9]+\.json$/u.test(relativePath)) {
      throw new Error(
        `accepted CTK3 distribution contains a stale authority manifest: ${relativePath}`,
      );
    }
    const absolutePath = resolve(root, ...relativePath.split("/"));
    const stats = await lstat(absolutePath);
    if (stats.isSymbolicLink()) {
      throw new Error(`accepted CTK3 distribution contains a symlink: ${relativePath}`);
    }
    if (stats.isDirectory()) {
      await walk(root, relativePath, files);
      continue;
    }
    if (!stats.isFile()) {
      throw new Error(
        `accepted CTK3 distribution contains a non-regular file: ${relativePath}`,
      );
    }
    const payload = await readFile(absolutePath);
    files.push({
      path: relativePath,
      size: payload.byteLength,
      sha256: createHash("sha256").update(payload).digest("hex"),
    });
  }
}

function requireCoreFiles(files) {
  const available = new Set(files.map((entry) => entry.path));
  for (const required of REQUIRED_FILES) {
    if (!available.has(required)) {
      throw new Error(`accepted CTK3 distribution is missing ${required}`);
    }
  }
}

function requireExactKeys(value, expected, description) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${description} must be an object`);
  }
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  if (JSON.stringify(actual) !== JSON.stringify(wanted)) {
    throw new Error(`${description} has unexpected keys: ${actual.join(",")}`);
  }
}

function requireSourceCommit(value, description) {
  if (typeof value !== "string" || !SOURCE_COMMIT_PATTERN.test(value)) {
    throw new Error(`${description} must be an exact lowercase 40-character SHA`);
  }
}

function requireRunAuthority(value, description) {
  if (typeof value !== "string" || !RUN_AUTHORITY_PATTERN.test(value)) {
    throw new Error(`${description} must be a canonical positive decimal string`);
  }
}

async function requireDirectory(path) {
  const stats = await lstat(path).catch((error) => {
    if (error?.code === "ENOENT") {
      throw new Error(`accepted CTK3 distribution is missing: ${path}`);
    }
    throw error;
  });
  if (!stats.isDirectory() || stats.isSymbolicLink()) {
    throw new Error(`accepted CTK3 distribution must be a directory: ${path}`);
  }
}

async function pathExists(path) {
  try {
    await lstat(path);
    return true;
  } catch (error) {
    if (error?.code === "ENOENT") return false;
    throw error;
  }
}

async function main(arguments_) {
  if (
    arguments_.length === 8 &&
    arguments_[0] === "--seal" &&
    arguments_[2] === "--source-commit" &&
    arguments_[4] === "--run-id" &&
    arguments_[6] === "--run-attempt"
  ) {
    const manifest = await sealAcceptedCtk3Dist(
      arguments_[1],
      arguments_[3],
      arguments_[5],
      arguments_[7],
    );
    console.log(
      `Accepted CTK3 distribution sealed: files=${manifest.files.length} source=${manifest.source_commit} run=${manifest.run_id}/${manifest.run_attempt}`,
    );
    return;
  }
  if (
    arguments_.length === 8 &&
    arguments_[0] === "--verify" &&
    arguments_[2] === "--expected-source-commit" &&
    arguments_[4] === "--expected-run-id" &&
    arguments_[6] === "--expected-run-attempt"
  ) {
    const manifest = await verifyAcceptedCtk3Dist(
      arguments_[1],
      arguments_[3],
      arguments_[5],
      arguments_[7],
    );
    console.log(
      `Accepted CTK3 distribution verified: files=${manifest.files.length} source=${manifest.source_commit} run=${manifest.run_id}/${manifest.run_attempt}`,
    );
    return;
  }
  throw new Error(
    "usage: accepted-ctk3-dist.mjs (--seal DIST --source-commit SHA --run-id ID --run-attempt ATTEMPT | --verify DIST --expected-source-commit SHA --expected-run-id ID --expected-run-attempt ATTEMPT)",
  );
}

const invokedPath = process.argv[1] ? resolve(process.argv[1]) : "";
if (invokedPath === fileURLToPath(import.meta.url)) {
  await main(process.argv.slice(2));
}
