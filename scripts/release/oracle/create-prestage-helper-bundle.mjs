import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { lstat, readFile, realpath } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

export const ORACLE_PRESTAGE_HELPER_SCHEMA =
  "clearra.oracle.prestage-helper-bundle.v1";

const SHA256 = /^[0-9a-f]{64}$/u;
const SOURCE_COMMIT = /^[0-9a-f]{40}$/u;
const OPERATIONS = new Set([
  "capture-prestage-authority",
  "cleanup-prestage-backup",
]);
const MAX_FILE_BYTES = 1024 * 1024;
const MAX_BUNDLE_BYTES = 4 * 1024 * 1024;
const FILE_CONTRACTS = Object.freeze([
  Object.freeze({
    path: "apps/clearra-discord-bot/scripts/capture-oracle-rollback-authority.mjs",
    imports: Object.freeze([
      "./oracle-runtime-authority.mjs",
      "./release-tree-digest.mjs",
      "node:child_process",
      "node:crypto",
      "node:fs",
      "node:path",
      "node:url",
      "node:util",
    ]),
  }),
  Object.freeze({
    path: "apps/clearra-discord-bot/scripts/oracle-runtime-authority.mjs",
    imports: Object.freeze([
      "../src/job-service/runtime-identity.mjs",
      "node:crypto",
    ]),
  }),
  Object.freeze({
    path: "apps/clearra-discord-bot/scripts/release-tree-digest.mjs",
    imports: Object.freeze([
      "node:crypto",
      "node:fs",
      "node:path",
      "node:url",
    ]),
  }),
  Object.freeze({
    path: "apps/clearra-discord-bot/src/job-service/runtime-identity.mjs",
    imports: Object.freeze([]),
  }),
]);

export async function createPrestageHelperBundleManifest({
  repositoryRoot,
  sourceCommit,
  deploymentNonce,
  operation,
}) {
  const root = resolve(requiredString(repositoryRoot, "repository root"));
  const rootMetadata = await lstat(root);
  if (!rootMetadata.isDirectory() || rootMetadata.isSymbolicLink()) {
    throw new Error("prestage helper repository root must be a regular directory");
  }
  if (await realpath(root) !== root) {
    throw new Error("prestage helper repository root must be canonical");
  }
  const acceptedSourceCommit = requiredPattern(
    sourceCommit,
    SOURCE_COMMIT,
    "prestage helper source commit",
  );
  const nonce = requiredPattern(
    deploymentNonce,
    SHA256,
    "prestage helper deployment nonce",
  );
  if (!OPERATIONS.has(operation)) {
    throw new Error("prestage helper operation is invalid");
  }
  requireAcceptedGitSource(root, acceptedSourceCommit);

  const files = [];
  let totalSize = 0;
  const bundleHash = createHash("sha256");
  bundleHash.update("clearra-oracle-prestage-helper-bundle-v1\0", "utf8");
  for (const contract of FILE_CONTRACTS) {
    const path = resolve(root, ...contract.path.split("/"));
    const metadata = await lstat(path);
    if (
      !metadata.isFile() ||
      metadata.isSymbolicLink() ||
      metadata.size <= 0 ||
      metadata.size > MAX_FILE_BYTES ||
      await realpath(path) !== path
    ) {
      throw new Error(`prestage helper file is not a bounded regular file: ${contract.path}`);
    }
    const bytes = await readFile(path);
    if (bytes.byteLength !== metadata.size) {
      throw new Error(`prestage helper file changed while reading: ${contract.path}`);
    }
    validateImportClosure(bytes, contract);
    requireAcceptedGitFile(root, acceptedSourceCommit, contract.path, bytes);
    totalSize += bytes.byteLength;
    if (!Number.isSafeInteger(totalSize) || totalSize > MAX_BUNDLE_BYTES) {
      throw new Error("prestage helper bundle exceeds its byte limit");
    }
    const digest = createHash("sha256").update(bytes).digest("hex");
    const entry = Object.freeze({
      path: contract.path,
      size: bytes.byteLength,
      sha256: digest,
      mode: "0644",
    });
    files.push(entry);
    bundleHash.update(contract.path, "utf8");
    bundleHash.update("\0", "utf8");
    bundleHash.update(String(bytes.byteLength), "utf8");
    bundleHash.update("\0", "utf8");
    bundleHash.update(bytes);
    bundleHash.update("\0", "utf8");
  }

  return Object.freeze({
    schema_id: ORACLE_PRESTAGE_HELPER_SCHEMA,
    source_commit: acceptedSourceCommit,
    deployment_nonce: nonce,
    operation,
    files: Object.freeze(files),
    file_count: files.length,
    total_size: totalSize,
    bundle_sha256: bundleHash.digest("hex"),
  });
}

function requireAcceptedGitSource(root, sourceCommit) {
  const gitRoot = runGit(root, ["rev-parse", "--show-toplevel"], "Git root")
    .toString("utf8").trim();
  const head = runGit(root, ["rev-parse", "HEAD"], "Git HEAD")
    .toString("utf8").trim();
  if (resolve(gitRoot) !== root || head !== sourceCommit) {
    throw new Error("prestage helper source is not the exact accepted Git checkout");
  }
}

function requireAcceptedGitFile(root, sourceCommit, path, bytes) {
  const status = runGit(
    root,
    ["status", "--porcelain=v1", "--untracked-files=all", "--", path],
    `Git status for ${path}`,
  );
  if (status.byteLength !== 0) {
    throw new Error(`prestage helper file differs from the accepted checkout: ${path}`);
  }
  const committed = runGit(root, ["show", `${sourceCommit}:${path}`], `Git blob for ${path}`);
  if (!committed.equals(bytes)) {
    throw new Error(`prestage helper file differs from the accepted Git blob: ${path}`);
  }
}

function runGit(root, arguments_, label) {
  const result = spawnSync("git", ["-C", root, ...arguments_], {
    shell: false,
    encoding: null,
    maxBuffer: 2 * 1024 * 1024,
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (result.error || result.status !== 0 || !Buffer.isBuffer(result.stdout)) {
    throw new Error(`prestage helper ${label} readback failed`);
  }
  return result.stdout;
}

function validateImportClosure(bytes, contract) {
  const text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  if (/\bimport\s*\(|\brequire\s*\(/u.test(text)) {
    throw new Error(`prestage helper uses a dynamic dependency: ${contract.path}`);
  }
  const imports = [
    ...[...text.matchAll(/\bfrom\s+["']([^"']+)["']/gu)].map((match) => match[1]),
    ...[...text.matchAll(/^\s*import\s+["']([^"']+)["']\s*;/gmu)].map((match) => match[1]),
  ].sort(compareText);
  const expected = [...contract.imports].sort(compareText);
  if (
    imports.length !== expected.length ||
    imports.some((specifier, index) => specifier !== expected[index])
  ) {
    throw new Error(`prestage helper import closure drifted: ${contract.path}`);
  }
  for (const specifier of imports) {
    if (!specifier.startsWith("node:") && !specifier.startsWith("./") && !specifier.startsWith("../")) {
      throw new Error(`prestage helper import is outside the closed bundle: ${contract.path}`);
    }
  }
}

function requiredString(value, label) {
  if (typeof value !== "string" || value.length === 0 || value.trim() !== value) {
    throw new Error(`${label} is invalid`);
  }
  return value;
}

function requiredPattern(value, pattern, label) {
  const text = requiredString(value, label);
  if (!pattern.test(text)) throw new Error(`${label} is invalid`);
  return text;
}

function compareText(left, right) {
  return left < right ? -1 : left > right ? 1 : 0;
}

async function main() {
  const { values } = parseArgs({
    options: {
      "repository-root": { type: "string" },
      "source-commit": { type: "string" },
      "deployment-nonce": { type: "string" },
      operation: { type: "string" },
    },
    strict: true,
  });
  const manifest = await createPrestageHelperBundleManifest({
    repositoryRoot: values["repository-root"],
    sourceCommit: values["source-commit"],
    deploymentNonce: values["deployment-nonce"],
    operation: values.operation,
  });
  process.stdout.write(`${JSON.stringify(manifest)}\n`);
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error(
      `oracle_prestage_helper_bundle=failed reason=${error instanceof Error ? error.message : String(error)}`,
    );
    process.exitCode = 2;
  });
}
