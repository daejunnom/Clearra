import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { lstat, readFile, readdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { isClearraWasmBuildContract } from "../tools/clearra-wasm-build-contract.mjs";

export const ACCEPTED_WASM_BUILD_RECEIPT = "clearra-accepted-wasm-build.v1.json";
export const ACCEPTED_WASM_BUILD_CONTRACT = "clearra.accepted-wasm-build.v1";

const SOURCE_COMMIT_PATTERN = /^[0-9a-f]{40}$/u;
const RUN_AUTHORITY_PATTERN = /^[1-9][0-9]{0,19}$/u;
const SHA256_PATTERN = /^[0-9a-f]{64}$/u;
const GENERATION_HEX_LENGTH = 24;
const WASM_MANIFEST = "clearra_wasm.manifest.json";
const PRODUCER_TOOLCHAIN_KEYS = Object.freeze([
  "cargo",
  "cmake",
  "node",
  "npm",
  "powershell",
  "rust",
  "wasm_bindgen",
]);

export async function sealAcceptedWasmBuild(
  buildPath,
  sourceCommit,
  runId,
  runAttempt,
  toolchains = collectAcceptedWasmProducerToolchains(),
) {
  const authority = validateAuthority(sourceCommit, runId, runAttempt);
  const tools = validateProducerToolchains(toolchains);
  const root = resolve(buildPath);
  await requireDirectory(root);
  const receiptPath = resolve(root, ACCEPTED_WASM_BUILD_RECEIPT);
  if (await pathExists(receiptPath)) {
    throw new Error(`accepted WASM build receipt already exists: ${receiptPath}`);
  }

  await validateWasmPayload(root, authority.sourceCommit);
  const files = await collectPayloadFiles(root);
  const manifest = files.find((entry) => entry.path === WASM_MANIFEST);
  if (!manifest) throw new Error(`accepted WASM build is missing ${WASM_MANIFEST}`);
  const receipt = {
    contract: ACCEPTED_WASM_BUILD_CONTRACT,
    source_commit: authority.sourceCommit,
    run_id: authority.runId,
    run_attempt: authority.runAttempt,
    manifest_sha256: manifest.sha256,
    payload_sha256: payloadSha256(files),
    toolchains: tools,
    files,
  };
  await writeFile(receiptPath, `${JSON.stringify(receipt, null, 2)}\n`, {
    encoding: "utf8",
    flag: "wx",
  });
  return receipt;
}

export async function verifyAcceptedWasmBuild(
  buildPath,
  expectedSourceCommit,
  expectedRunId,
  expectedRunAttempt,
) {
  const expected = validateAuthority(
    expectedSourceCommit,
    expectedRunId,
    expectedRunAttempt,
  );
  const root = resolve(buildPath);
  await requireDirectory(root);
  const receiptPath = resolve(root, ACCEPTED_WASM_BUILD_RECEIPT);
  const receiptStats = await lstat(receiptPath).catch((error) => {
    if (error?.code === "ENOENT") {
      throw new Error(`accepted WASM build receipt is missing: ${receiptPath}`);
    }
    throw error;
  });
  if (!receiptStats.isFile() || receiptStats.isSymbolicLink()) {
    throw new Error("accepted WASM build receipt must be a regular file");
  }

  let receipt;
  try {
    receipt = JSON.parse(await readFile(receiptPath, "utf8"));
  } catch (error) {
    throw new Error(`accepted WASM build receipt is not valid JSON: ${error.message}`);
  }
  requireExactKeys(
    receipt,
    [
      "contract",
      "files",
      "manifest_sha256",
      "payload_sha256",
      "run_attempt",
      "run_id",
      "source_commit",
      "toolchains",
    ],
    "accepted WASM build receipt",
  );
  if (receipt.contract !== ACCEPTED_WASM_BUILD_CONTRACT) {
    throw new Error(`accepted WASM build contract mismatch: ${receipt.contract}`);
  }
  assertAuthorityMatches(receipt, expected);
  const tools = validateProducerToolchains(receipt.toolchains);
  validateManifestFiles(receipt.files);
  if (!SHA256_PATTERN.test(receipt.manifest_sha256 ?? "")) {
    throw new Error("accepted WASM manifest SHA-256 is invalid");
  }
  if (!SHA256_PATTERN.test(receipt.payload_sha256 ?? "")) {
    throw new Error("accepted WASM payload SHA-256 is invalid");
  }
  if (payloadSha256(receipt.files) !== receipt.payload_sha256) {
    throw new Error("accepted WASM payload digest differs from its sealed file set");
  }

  const actualFiles = await collectPayloadFiles(root);
  if (JSON.stringify(actualFiles) !== JSON.stringify(receipt.files)) {
    throw new Error("accepted WASM build does not match its closed regular-file set and hashes");
  }
  const manifest = actualFiles.find((entry) => entry.path === WASM_MANIFEST);
  if (!manifest || manifest.sha256 !== receipt.manifest_sha256) {
    throw new Error("accepted WASM manifest differs from its sealed digest");
  }
  await validateWasmPayload(root, expected.sourceCommit);
  return Object.freeze({ ...receipt, toolchains: tools });
}

export function collectAcceptedWasmProducerToolchains(dependencies = {}) {
  const run = dependencies.run ?? runVersionCommand;
  const platform = dependencies.platform ?? process.platform;
  const npmInvocation = platform === "win32"
    ? ["cmd.exe", ["/d", "/s", "/c", "npm.cmd --version"], "npm"]
    : ["npm", ["--version"], "npm"];
  const invocations = new Map([
    ["cargo", ["cargo", ["--version"], "cargo"]],
    ["cmake", ["cmake", ["--version"], "cmake"]],
    ["node", ["node", ["--version"], "node"]],
    ["npm", npmInvocation],
    ["powershell", [
      "powershell",
      ["-NoProfile", "-Command", "$PSVersionTable.PSVersion.ToString()"],
      "PowerShell",
    ]],
    ["rust", ["rustc", ["--version"], "rustc"]],
    ["wasm_bindgen", ["wasm-bindgen", ["--version"], "wasm-bindgen"]],
  ]);
  const toolchains = {};
  for (const key of PRODUCER_TOOLCHAIN_KEYS) {
    const [command, arguments_, label] = invocations.get(key);
    toolchains[key] = firstLine(run(command, arguments_), label);
  }
  return Object.freeze(toolchains);
}

function validateAuthority(sourceCommit, runId, runAttempt) {
  requirePattern(sourceCommit, SOURCE_COMMIT_PATTERN, "accepted WASM source commit");
  requirePattern(runId, RUN_AUTHORITY_PATTERN, "accepted WASM run ID");
  requirePattern(runAttempt, RUN_AUTHORITY_PATTERN, "accepted WASM run attempt");
  return Object.freeze({ sourceCommit, runId, runAttempt });
}

function assertAuthorityMatches(receipt, expected) {
  for (const [field, expectedValue, label] of [
    ["source_commit", expected.sourceCommit, "source commit"],
    ["run_id", expected.runId, "run ID"],
    ["run_attempt", expected.runAttempt, "run attempt"],
  ]) {
    if (receipt[field] !== expectedValue) {
      throw new Error(
        `accepted WASM ${label} mismatch: expected ${expectedValue}, received ${receipt[field]}`,
      );
    }
  }
}

function validateProducerToolchains(value) {
  requireExactKeys(value, PRODUCER_TOOLCHAIN_KEYS, "accepted WASM producer toolchains");
  const tools = {};
  for (const key of PRODUCER_TOOLCHAIN_KEYS) {
    if (typeof value[key] !== "string" || value[key].trim().length === 0) {
      throw new Error(`accepted WASM producer toolchain ${key} must be non-empty`);
    }
    tools[key] = value[key];
  }
  return Object.freeze(tools);
}

async function validateWasmPayload(root, expectedSourceCommit) {
  const manifestBytes = await readRequiredFile(resolve(root, WASM_MANIFEST), "WASM manifest");
  let manifest;
  try {
    manifest = JSON.parse(manifestBytes.toString("utf8"));
  } catch (error) {
    throw new Error(`accepted WASM manifest is not valid JSON: ${error.message}`);
  }
  requireExactKeys(
    manifest,
    ["bindings", "build", "schema_version", "wasm"],
    "accepted WASM manifest",
  );
  if (manifest.schema_version !== 1 || !isClearraWasmBuildContract(manifest.build)) {
    throw new Error("accepted WASM manifest build contract is invalid");
  }
  const identity = manifest.build.runtime_identity;
  if (
    identity.source_commit !== expectedSourceCommit ||
    identity.engine_build_id !== expectedSourceCommit
  ) {
    throw new Error("accepted WASM manifest does not match the expected source identity");
  }
  const bindings = validateArtifactDescriptor(manifest.bindings, "bindings", "clearra_wasm", ".js");
  const wasm = validateArtifactDescriptor(manifest.wasm, "WASM", "clearra_wasm_bg", ".wasm");
  await Promise.all([
    validateArtifactFile(root, bindings, "bindings"),
    validateArtifactFile(root, wasm, "WASM"),
    validateAliasFile(root, "clearra_wasm.js", bindings, "bindings"),
    validateAliasFile(root, "clearra_wasm_bg.wasm", wasm, "WASM"),
  ]);
  return manifest;
}

function validateArtifactDescriptor(value, label, basename, extension) {
  requireExactKeys(value, ["bytes", "path", "sha256"], `accepted WASM ${label} descriptor`);
  if (
    typeof value.path !== "string" ||
    value.path !== `${basename}.${String(value.sha256).slice(0, GENERATION_HEX_LENGTH)}${extension}` ||
    !Number.isSafeInteger(value.bytes) ||
    value.bytes <= 0 ||
    !SHA256_PATTERN.test(value.sha256 ?? "")
  ) {
    throw new Error(`accepted WASM ${label} descriptor is invalid`);
  }
  return value;
}

async function validateArtifactFile(root, descriptor, label) {
  const payload = await readRequiredFile(resolve(root, descriptor.path), `WASM ${label}`);
  if (
    payload.byteLength !== descriptor.bytes ||
    sha256(payload) !== descriptor.sha256
  ) {
    throw new Error(`accepted WASM ${label} differs from its manifest`);
  }
}

async function validateAliasFile(root, alias, descriptor, label) {
  const payload = await readRequiredFile(resolve(root, alias), `WASM ${label} alias`);
  if (payload.byteLength !== descriptor.bytes || sha256(payload) !== descriptor.sha256) {
    throw new Error(`accepted WASM ${label} alias differs from its versioned payload`);
  }
}

async function collectPayloadFiles(root) {
  const entries = await readdir(root, { withFileTypes: true });
  entries.sort((left, right) => left.name.localeCompare(right.name, "en"));
  const files = [];
  for (const entry of entries) {
    if (entry.name === ACCEPTED_WASM_BUILD_RECEIPT) continue;
    if (/^clearra-accepted-wasm-build\.v[0-9]+\.json$/u.test(entry.name)) {
      throw new Error(`accepted WASM build contains a stale authority receipt: ${entry.name}`);
    }
    if (!entry.isFile() || entry.isSymbolicLink()) {
      throw new Error(`accepted WASM build contains a non-regular entry: ${entry.name}`);
    }
    const payload = await readFile(resolve(root, entry.name));
    files.push({
      path: entry.name,
      size: payload.byteLength,
      sha256: sha256(payload),
    });
  }
  if (files.length === 0) throw new Error("accepted WASM build is empty");
  return files;
}

function validateManifestFiles(files) {
  if (!Array.isArray(files) || files.length === 0) {
    throw new Error("accepted WASM receipt files must be a non-empty array");
  }
  let previousPath = "";
  for (const [index, file] of files.entries()) {
    requireExactKeys(file, ["path", "sha256", "size"], `accepted WASM file ${index}`);
    if (
      typeof file.path !== "string" ||
      file.path.length === 0 ||
      file.path.includes("/") ||
      file.path.includes("\\") ||
      (previousPath.length > 0 && previousPath.localeCompare(file.path, "en") >= 0)
    ) {
      throw new Error("accepted WASM receipt paths must be flat, safe, unique, and sorted");
    }
    previousPath = file.path;
    if (!Number.isSafeInteger(file.size) || file.size <= 0) {
      throw new Error(`accepted WASM receipt file ${file.path} has an invalid size`);
    }
    if (!SHA256_PATTERN.test(file.sha256 ?? "")) {
      throw new Error(`accepted WASM receipt file ${file.path} has an invalid SHA-256`);
    }
  }
}

function payloadSha256(files) {
  return sha256(Buffer.from(JSON.stringify(files), "utf8"));
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
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
}

async function requireDirectory(path) {
  const stats = await lstat(path).catch((error) => {
    if (error?.code === "ENOENT") throw new Error(`accepted WASM build is missing: ${path}`);
    throw error;
  });
  if (!stats.isDirectory() || stats.isSymbolicLink()) {
    throw new Error(`accepted WASM build must be a directory: ${path}`);
  }
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

async function pathExists(path) {
  try {
    await lstat(path);
    return true;
  } catch (error) {
    if (error?.code === "ENOENT") return false;
    throw error;
  }
}

function runVersionCommand(command, arguments_) {
  const result = spawnSync(command, arguments_, {
    encoding: "utf8",
    windowsHide: true,
    shell: false,
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (result.error || result.status !== 0) {
    throw new Error(`accepted WASM producer toolchain command failed: ${command}`);
  }
  return result.stdout;
}

function firstLine(output, label) {
  const value = String(output ?? "").split(/\r?\n/u)[0]?.trim();
  if (!value) throw new Error(`accepted WASM producer ${label} version is empty`);
  return value;
}

async function main(arguments_) {
  if (
    arguments_.length === 8 &&
    arguments_[0] === "--seal" &&
    arguments_[2] === "--source-commit" &&
    arguments_[4] === "--run-id" &&
    arguments_[6] === "--run-attempt"
  ) {
    const receipt = await sealAcceptedWasmBuild(
      arguments_[1],
      arguments_[3],
      arguments_[5],
      arguments_[7],
    );
    console.log(
      `Accepted WASM build sealed: files=${receipt.files.length} payload_sha256=${receipt.payload_sha256} source=${receipt.source_commit} run=${receipt.run_id}/${receipt.run_attempt}`,
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
    const receipt = await verifyAcceptedWasmBuild(
      arguments_[1],
      arguments_[3],
      arguments_[5],
      arguments_[7],
    );
    console.log(
      `Accepted WASM build verified: files=${receipt.files.length} payload_sha256=${receipt.payload_sha256} source=${receipt.source_commit} run=${receipt.run_id}/${receipt.run_attempt}`,
    );
    return;
  }
  throw new Error(
    "usage: accepted-wasm-build.mjs (--seal DIR --source-commit SHA --run-id ID --run-attempt ATTEMPT | --verify DIR --expected-source-commit SHA --expected-run-id ID --expected-run-attempt ATTEMPT)",
  );
}

const invokedPath = process.argv[1] ? resolve(process.argv[1]) : "";
if (invokedPath === fileURLToPath(import.meta.url)) {
  await main(process.argv.slice(2));
}
