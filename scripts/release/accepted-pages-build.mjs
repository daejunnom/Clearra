import { createHash } from "node:crypto";
import {
  lstat,
  readFile,
  readdir,
  writeFile,
} from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { verifyAcceptedWasmBuild } from "./accepted-wasm-build.mjs";

export const PAGES_IDENTITY_FILE = "clearra-build-identity.json";
export const PAGES_IDENTITY_SCHEMA = "clearra.pages.identity.v2";

const COMMIT_PATTERN = /^[0-9a-f]{40}$/u;
const DECIMAL_ID_PATTERN = /^[1-9][0-9]*$/u;
const BASE_PATH_PATTERN = /^\/[A-Za-z0-9._-]+$/u;
const VERSION_PATTERN = /^[0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.-]+)?$/u;
const SHA256_PATTERN = /^[0-9a-f]{64}$/u;
const CONTRACT_SCHEMA_VERSION = "clearra.search.contract.v2";
const SUPPLY_SEMANTICS_ID = "clearra.supply.projected-terminal-lookahead.v1";
const ARTIFACT_SCHEMA_VERSION = "clearra.solution-data.v1";

export async function stampAcceptedPagesBuild(buildPath, authority) {
  const expected = validateAuthority(authority);
  const root = resolve(buildPath);
  await requireDirectory(root);
  const identityPath = resolve(root, PAGES_IDENTITY_FILE);
  if (await pathExists(identityPath)) {
    throw new Error(`accepted Pages identity already exists: ${identityPath}`);
  }
  await validateDeployableSurfaces(root, expected);
  const files = await collectPayloadFiles(root);
  const identity = identityFrom(expected, files);
  await writeFile(identityPath, `${JSON.stringify(identity, null, 2)}\n`, {
    encoding: "utf8",
    flag: "wx",
  });
  return identity;
}

export async function verifyAcceptedPagesBuild(buildPath, authority) {
  const expected = validateAuthority(authority);
  const root = resolve(buildPath);
  await requireDirectory(root);
  const identityPath = resolve(root, PAGES_IDENTITY_FILE);
  const identityStats = await lstat(identityPath).catch((error) => {
    if (error?.code === "ENOENT") {
      throw new Error(`accepted Pages identity is missing: ${identityPath}`);
    }
    throw error;
  });
  if (!identityStats.isFile() || identityStats.isSymbolicLink()) {
    throw new Error("accepted Pages identity must be a regular file");
  }
  let identity;
  try {
    identity = JSON.parse(await readFile(identityPath, "utf8"));
  } catch (error) {
    throw new Error(`accepted Pages identity is not valid JSON: ${error.message}`);
  }
  requireExactKeys(
    identity,
    [
      "acceptedRunAttempt",
      "acceptedRunId",
      "artifactSchemaVersion",
      "basePath",
      "contractSchemaVersion",
      "engineBuildId",
      "files",
      "schema",
      "sourceCommit",
      "supplySemanticsId",
      "version",
    ],
    "accepted Pages identity",
  );
  const canonical = identityFrom(expected, identity.files);
  for (const key of Object.keys(canonical)) {
    if (key === "files") continue;
    if (identity[key] !== canonical[key]) {
      throw new Error(
        `accepted Pages identity ${key} mismatch: expected ${canonical[key]}, received ${identity[key]}`,
      );
    }
  }
  validateManifestFiles(identity.files);
  await validateDeployableSurfaces(root, expected);
  const actualFiles = await collectPayloadFiles(root);
  if (JSON.stringify(actualFiles) !== JSON.stringify(identity.files)) {
    throw new Error(
      "accepted Pages build does not match its closed regular-file set and hashes",
    );
  }
  return identity;
}

function identityFrom(authority, files) {
  return {
    schema: PAGES_IDENTITY_SCHEMA,
    sourceCommit: authority.sourceCommit,
    engineBuildId: authority.sourceCommit,
    contractSchemaVersion: CONTRACT_SCHEMA_VERSION,
    supplySemanticsId: SUPPLY_SEMANTICS_ID,
    artifactSchemaVersion: ARTIFACT_SCHEMA_VERSION,
    version: authority.version,
    acceptedRunId: authority.acceptedRunId,
    acceptedRunAttempt: authority.acceptedRunAttempt,
    basePath: authority.basePath,
    files,
  };
}

function validateAuthority(authority) {
  if (authority === null || typeof authority !== "object" || Array.isArray(authority)) {
    throw new Error("accepted Pages authority must be an object");
  }
  const sourceCommit = requirePattern(
    authority.sourceCommit,
    COMMIT_PATTERN,
    "accepted Pages source commit",
  );
  const acceptedRunId = requirePattern(
    String(authority.acceptedRunId ?? ""),
    DECIMAL_ID_PATTERN,
    "accepted Pages run ID",
  );
  const acceptedRunAttempt = requirePattern(
    String(authority.acceptedRunAttempt ?? ""),
    DECIMAL_ID_PATTERN,
    "accepted Pages run attempt",
  );
  const basePath = requirePattern(
    authority.basePath,
    BASE_PATH_PATTERN,
    "accepted Pages base path",
  );
  const version = requirePattern(
    authority.version,
    VERSION_PATTERN,
    "accepted Pages version",
  );
  return Object.freeze({
    sourceCommit,
    acceptedRunId,
    acceptedRunAttempt,
    basePath,
    version,
  });
}

async function validateDeployableSurfaces(root, authority) {
  const indexPath = resolve(root, "index.html");
  const fallbackPath = resolve(root, "404.html");
  const [index, fallback] = await Promise.all([
    readRequiredFile(indexPath, "Pages index"),
    readRequiredFile(fallbackPath, "Pages fallback"),
  ]);
  if (!index.equals(fallback)) {
    throw new Error("accepted Pages 404 fallback must exactly match index.html");
  }
  const html = index.toString("utf8");
  if (!html.includes(`${authority.basePath}/_app/`)) {
    throw new Error(
      `accepted Pages HTML does not prove base path ${authority.basePath}`,
    );
  }

  await verifyAcceptedWasmBuild(
    resolve(root, "wasm"),
    authority.sourceCommit,
    authority.acceptedRunId,
    authority.acceptedRunAttempt,
  );

  const manifestPath = resolve(root, "wasm", "clearra_wasm.manifest.json");
  let manifest;
  try {
    manifest = JSON.parse(await readFile(manifestPath, "utf8"));
  } catch (error) {
    throw new Error(`accepted Pages WASM manifest is invalid: ${error.message}`);
  }
  const runtime = manifest?.build?.runtime_identity;
  if (
    runtime?.source_commit !== authority.sourceCommit ||
    runtime?.engine_build_id !== authority.sourceCommit ||
    runtime?.contract_schema_version !== CONTRACT_SCHEMA_VERSION ||
    runtime?.supply_semantics_id !== SUPPLY_SEMANTICS_ID ||
    runtime?.artifact_schema_version !== ARTIFACT_SCHEMA_VERSION
  ) {
    throw new Error("accepted Pages WASM manifest has a mismatched product identity");
  }
  await Promise.all([
    validateWasmArtifact(root, manifest?.bindings, "bindings", /\.js$/u),
    validateWasmArtifact(root, manifest?.wasm, "wasm", /\.wasm$/u),
  ]);
}

async function validateWasmArtifact(root, artifact, label, suffixPattern) {
  if (
    artifact === null ||
    typeof artifact !== "object" ||
    typeof artifact.path !== "string" ||
    artifact.path.includes("/") ||
    artifact.path.includes("\\") ||
    !artifact.path.startsWith("clearra_wasm") ||
    !suffixPattern.test(artifact.path) ||
    !Number.isSafeInteger(artifact.bytes) ||
    artifact.bytes <= 0 ||
    typeof artifact.sha256 !== "string" ||
    !SHA256_PATTERN.test(artifact.sha256)
  ) {
    throw new Error(`accepted Pages WASM ${label} descriptor is invalid`);
  }
  const payload = await readRequiredFile(
    resolve(root, "wasm", artifact.path),
    `Pages WASM ${label}`,
  );
  const hash = createHash("sha256").update(payload).digest("hex");
  if (payload.byteLength !== artifact.bytes || hash !== artifact.sha256) {
    throw new Error(`accepted Pages WASM ${label} differs from its manifest`);
  }
}

async function collectPayloadFiles(root) {
  const files = [];
  await walk(root, "", files);
  files.sort((left, right) => left.path.localeCompare(right.path, "en"));
  if (files.length === 0) throw new Error("accepted Pages build is empty");
  return files;
}

async function walk(root, relativeDirectory, files) {
  const directory = relativeDirectory
    ? resolve(root, ...relativeDirectory.split("/"))
    : root;
  const entries = await readdir(directory, { withFileTypes: true });
  entries.sort((left, right) => left.name.localeCompare(right.name, "en"));
  for (const entry of entries) {
    const relativePath = relativeDirectory
      ? `${relativeDirectory}/${entry.name}`
      : entry.name;
    if (relativePath === PAGES_IDENTITY_FILE) continue;
    const absolutePath = resolve(root, ...relativePath.split("/"));
    const stats = await lstat(absolutePath);
    if (stats.isSymbolicLink()) {
      throw new Error(
        `accepted Pages build contains a symlink or reparse point: ${relativePath}`,
      );
    }
    if (stats.isDirectory()) {
      await walk(root, relativePath, files);
      continue;
    }
    if (!stats.isFile()) {
      throw new Error(
        `accepted Pages build contains a non-regular file: ${relativePath}`,
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

function validateManifestFiles(files) {
  if (!Array.isArray(files) || files.length === 0) {
    throw new Error("accepted Pages identity files must be a non-empty array");
  }
  let previousPath = "";
  for (const [index, file] of files.entries()) {
    requireExactKeys(
      file,
      ["path", "sha256", "size"],
      `accepted Pages identity file ${index}`,
    );
    if (
      typeof file.path !== "string" ||
      file.path.length === 0 ||
      file.path.includes("\\") ||
      file.path.startsWith("/") ||
      file.path.split("/").includes("..") ||
      (previousPath.length > 0 &&
        previousPath.localeCompare(file.path, "en") >= 0)
    ) {
      throw new Error("accepted Pages identity paths must be safe, unique, and sorted");
    }
    previousPath = file.path;
    if (!Number.isSafeInteger(file.size) || file.size < 0) {
      throw new Error(`accepted Pages identity file ${file.path} has an invalid size`);
    }
    if (typeof file.sha256 !== "string" || !SHA256_PATTERN.test(file.sha256)) {
      throw new Error(`accepted Pages identity file ${file.path} has an invalid SHA-256`);
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

function requirePattern(value, pattern, description) {
  if (typeof value !== "string" || !pattern.test(value)) {
    throw new Error(`${description} has an invalid format`);
  }
  return value;
}

async function readRequiredFile(path, description) {
  const stats = await lstat(path).catch((error) => {
    if (error?.code === "ENOENT") throw new Error(`${description} is missing: ${path}`);
    throw error;
  });
  if (!stats.isFile() || stats.isSymbolicLink() || stats.size <= 0) {
    throw new Error(`${description} must be a non-empty regular file: ${path}`);
  }
  return readFile(path);
}

async function requireDirectory(path) {
  const stats = await lstat(path).catch((error) => {
    if (error?.code === "ENOENT") throw new Error(`accepted Pages build is missing: ${path}`);
    throw error;
  });
  if (!stats.isDirectory() || stats.isSymbolicLink()) {
    throw new Error(`accepted Pages build must be a directory: ${path}`);
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

function parseArguments(arguments_) {
  const mode = arguments_[0];
  if (!new Set(["--stamp", "--verify"]).has(mode) || arguments_.length !== 12) {
    throw new Error(
      "usage: accepted-pages-build.mjs (--stamp|--verify) BUILD --source-commit SHA --accepted-run-id ID --accepted-run-attempt ATTEMPT --base-path PATH --version VERSION",
    );
  }
  const expectedFlags = [
    "--source-commit",
    "--accepted-run-id",
    "--accepted-run-attempt",
    "--base-path",
    "--version",
  ];
  for (let index = 0; index < expectedFlags.length; index += 1) {
    if (arguments_[2 + index * 2] !== expectedFlags[index]) {
      throw new Error(`accepted Pages argument ${expectedFlags[index]} is missing`);
    }
  }
  return {
    mode,
    buildPath: arguments_[1],
    authority: {
      sourceCommit: arguments_[3],
      acceptedRunId: arguments_[5],
      acceptedRunAttempt: arguments_[7],
      basePath: arguments_[9],
      version: arguments_[11],
    },
  };
}

async function main(arguments_) {
  const parsed = parseArguments(arguments_);
  const identity = parsed.mode === "--stamp"
    ? await stampAcceptedPagesBuild(parsed.buildPath, parsed.authority)
    : await verifyAcceptedPagesBuild(parsed.buildPath, parsed.authority);
  console.log(
    `Accepted Pages build ${parsed.mode === "--stamp" ? "stamped" : "verified"}: files=${identity.files.length} source=${identity.sourceCommit} run=${identity.acceptedRunId}/${identity.acceptedRunAttempt} base=${identity.basePath}`,
  );
}

const invokedPath = process.argv[1] ? resolve(process.argv[1]) : "";
if (invokedPath === fileURLToPath(import.meta.url)) {
  await main(process.argv.slice(2));
}
