import { createHash } from "node:crypto";
import { constants } from "node:fs";
import { lstat, mkdtemp, open, realpath, rmdir, unlink } from "node:fs/promises";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { crc32 } from "node:zlib";
import { inspectCtk3Exact } from "ctk3";

const CONTRACT = "clearra.discord-solution-artifact.v1";
const PREFIX = "clearra-discord-solutions-";
const SUPPORTED = new Set(["pc-scenario", "build-probability"]);

export function usesNativeSolutionArtifact(arguments_) {
  const command = arguments_?.[0];
  // Typed PC products already expose a bounded, source-owned result. Ordinary
  // materialized solution sets use the existing native file encoder instead
  // of copying every key into stdout and encoding it again in the Node host.
  // Percent owns a coverage summary and legacy Cover owns no solution keys;
  // requesting an artifact there would change a valid response into an error.
  return SUPPORTED.has(command) || command === "pc" &&
    (arguments_.length === 1 || arguments_[1]?.startsWith("--"));
}

export async function createSolutionArtifactPlan(arguments_, maximumBytes) {
  if (!usesNativeSolutionArtifact(arguments_)) return null;
  if (!Number.isSafeInteger(maximumBytes) || maximumBytes < 1) {
    throw new Error("The Discord solution artifact limit is invalid.");
  }
  const directory = await mkdtemp(join(tmpdir(), PREFIX));
  const canonicalDirectory = await realpath(directory);
  if (resolve(directory) !== resolve(canonicalDirectory)) {
    await rmdir(directory).catch(() => {});
    throw new Error("The Discord solution artifact directory is not canonical.");
  }
  return Object.freeze({ directory, canonicalDirectory,
    outputPath: join(directory, "solutions.ctk3"), maximumBytes });
}

export async function readSolutionArtifact(plan, document) {
  const receipt = document?.solution_artifact;
  if (!receipt || typeof receipt !== "object" || Array.isArray(receipt) ||
      receipt.schema !== "solution-set-artifact.v2" || receipt.encoding !== "ctk3" ||
      !Number.isSafeInteger(receipt.solution_count) || receipt.solution_count < 0 ||
      !Number.isSafeInteger(receipt.annotated_solution_count) || receipt.annotated_solution_count < 0 ||
      receipt.annotated_solution_count > receipt.solution_count) {
    throw new Error("Clearra omitted its native solution artifact receipt.");
  }
  const declared = document?.summary?.normalized_unique_solution_count;
  if (declared !== undefined && (!/^(0|[1-9][0-9]*)$/u.test(String(declared)) ||
      BigInt(declared) !== BigInt(receipt.solution_count))) {
    throw new Error("Clearra solution artifact does not match its source count.");
  }
  if (receipt.status === "empty") {
    if (receipt.solution_count !== 0 || receipt.annotated_solution_count !== 0 ||
        receipt.bytes !== 0 || receipt.target_owned !== false ||
        await lstat(plan.outputPath).then(() => true, (error) => {
          if (error?.code === "ENOENT") return false;
          throw error;
        })) throw new Error("Clearra returned an inconsistent empty solution artifact.");
    return null;
  }
  if (receipt.status !== "committed" || receipt.target_owned !== true ||
      receipt.compression !== "ctk3-revision-native" || receipt.solution_count < 1 ||
      !Number.isSafeInteger(receipt.bytes) || receipt.bytes < 1 || receipt.bytes > plan.maximumBytes) {
    throw new Error("Clearra solution artifact has an invalid commit receipt.");
  }
  if (await realpath(plan.directory) !== plan.canonicalDirectory) {
    throw new Error("The Discord solution artifact directory changed.");
  }
  const before = await lstat(plan.outputPath, { bigint: true });
  if (!before.isFile() || before.isSymbolicLink() || before.size !== BigInt(receipt.bytes)) {
    throw new Error("Clearra did not produce one bounded regular solution artifact.");
  }
  const file = await open(plan.outputPath, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0));
  let bytes;
  try {
    const opened = await file.stat({ bigint: true });
    if (!sameFile(before, opened) || !opened.isFile()) throw new Error("The solution artifact changed before reading.");
    if (process.platform === "linux" && (receipt.file_identity_kind !== "linux-device-inode" ||
        receipt.file_identity !== `${opened.dev.toString(16).padStart(16, "0")}:${opened.ino.toString(16).padStart(16, "0")}`)) {
      throw new Error("Clearra solution artifact file identity does not match its receipt.");
    }
    const buffer = Buffer.allocUnsafe(receipt.bytes + 1);
    let offset = 0;
    while (offset < buffer.length) {
      const read = await file.read(buffer, offset, buffer.length - offset, offset);
      if (read.bytesRead === 0) break;
      offset += read.bytesRead;
    }
    bytes = buffer.subarray(0, offset);
    const after = await file.stat({ bigint: true });
    if (!sameFile(opened, after) || opened.mtimeNs !== after.mtimeNs || opened.ctimeNs !== after.ctimeNs) {
      throw new Error("The solution artifact changed while reading.");
    }
  } finally { await file.close(); }
  if (bytes.length !== receipt.bytes || receipt.checksum !== `crc32:${crc32(bytes).toString(16).padStart(8, "0")}`) {
    throw new Error("Clearra solution artifact checksum does not match its receipt.");
  }
  const source = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  const info = inspectCtk3Exact(source);
  if (info.pageCount !== receipt.solution_count) {
    throw new Error("Clearra solution artifact page count does not match its receipt.");
  }
  return Object.freeze({ contract: CONTRACT, mediaType: "text/plain; charset=utf-8",
    filename: "clearra-result.ctk3", byteLength: bytes.length,
    sha256: createHash("sha256").update(bytes).digest("hex"), source,
    solutionCount: info.pageCount, complete: true });
}

function sameFile(left, right) {
  return left.dev === right.dev && left.ino === right.ino && left.size === right.size;
}

export function validateSolutionTransportArtifact(value, maximumBytes) {
  if (!value || value.contract !== CONTRACT || value.mediaType !== "text/plain; charset=utf-8" ||
      value.filename !== "clearra-result.ctk3" || value.complete !== true ||
      typeof value.source !== "string" || !Number.isSafeInteger(value.byteLength) ||
      value.byteLength < 1 || value.byteLength > maximumBytes ||
      !Number.isSafeInteger(value.solutionCount) || value.solutionCount < 1 ||
      typeof value.sha256 !== "string" || !/^[0-9a-f]{64}$/u.test(value.sha256)) {
    throw new Error("Clearra returned an invalid solution artifact transport.");
  }
  const bytes = Buffer.from(value.source, "utf8");
  if (bytes.length !== value.byteLength || createHash("sha256").update(bytes).digest("hex") !== value.sha256 ||
      inspectCtk3Exact(value.source).pageCount !== value.solutionCount) {
    throw new Error("Clearra returned a mismatched solution artifact transport.");
  }
  return Object.freeze({ ...value });
}

export async function cleanupSolutionArtifactPlan(plan) {
  if (!plan) return;
  const metadata = await lstat(plan.directory).catch((error) => {
    if (error?.code === "ENOENT") return null;
    throw error;
  });
  if (!metadata) return;
  if (!metadata.isDirectory() || metadata.isSymbolicLink() ||
      await realpath(plan.directory) !== plan.canonicalDirectory ||
      dirname(plan.outputPath) !== plan.directory || !basename(plan.directory).startsWith(PREFIX)) {
    throw new Error("The Discord solution artifact directory changed before cleanup.");
  }
  await unlink(plan.outputPath).catch((error) => { if (error?.code !== "ENOENT") throw error; });
  await rmdir(plan.directory);
}
