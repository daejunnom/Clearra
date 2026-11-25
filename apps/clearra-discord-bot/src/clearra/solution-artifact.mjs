import { createHash } from "node:crypto";
import { constants } from "node:fs";
import { lstat, mkdtemp, open, realpath, rmdir, unlink } from "node:fs/promises";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { crc32 } from "node:zlib";

const CONTRACT = "clearra.discord-solution-artifact.v1";
const PREFIX = "clearra-discord-solutions-";
const SUPPORTED = new Set(["pc-scenario", "build-probability"]);
const MAX_SEGMENT_PAGES = 4096;
const MAX_DOCUMENT_PAGES = 1_048_576;
const MAX_SEGMENT_CHARACTERS = Math.ceil((16 * 1024 * 1024 * 8) / 6);
const CTK64_ALPHABET = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

// The native solution encoder emits CTK64 segments or a CTK64 bundle. Inspect
// the same common headers as the document codec, without importing its Fumen
// dependency or decoding/materializing pages in the job service. File CRC32,
// identity and transport SHA-256 remain the integrity authorities below.
function inspectNativeSolutionEnvelope(input, expectedPages) {
  const source = input.trim();
  const bundled = source.slice(0, 6).toLowerCase() === "ctk3b_";
  if (!bundled && source.slice(0, 5).toLowerCase() !== "ctk3_") {
    throw new Error("Clearra returned an invalid native CTK3 envelope.");
  }
  let cursor = bundled ? 6 : 5;
  let width = null;
  let pageCount = 0;
  let segments = 0;
  while (cursor <= source.length) {
    const separator = bundled ? source.indexOf(".", cursor) : -1;
    const end = separator < 0 ? source.length : separator;
    const length = end - cursor;
    if (length < 1 || length > MAX_SEGMENT_CHARACTERS || length % 4 === 1) {
      throw new Error("Clearra returned an invalid native CTK3 segment length.");
    }
    const payload = source.slice(cursor, end);
    if (!/^[A-Za-z0-9_-]+$/u.test(payload)) {
      throw new Error("Clearra returned an invalid native CTK3 alphabet.");
    }
    const unusedBits = length % 4 === 2 ? 4 : length % 4 === 3 ? 2 : 0;
    if (CTK64_ALPHABET.indexOf(payload.at(-1)) & ((1 << unusedBits) - 1)) {
      throw new Error("Clearra returned noncanonical native CTK3 padding.");
    }
    // At most eight decoded bytes cover every supported header. Exclude the
    // trailing CRC16 bytes from the readable body even for truncated segments.
    const bodyBytes = Math.floor(length * 6 / 8) - 2;
    const header = Buffer.from(payload.slice(0, 12), "base64url").subarray(0, 8);
    const availableBits = Math.min(header.length, bodyBytes) * 8;
    let bitOffset = 0;
    const readBits = (count) => {
      if (bitOffset + count > availableBits) {
        throw new Error("Clearra returned a truncated native CTK3 header.");
      }
      let value = 0;
      for (let bit = 0; bit < count; bit++, bitOffset++) {
        value += ((header[Math.floor(bitOffset / 8)] >>> (bitOffset % 8)) & 1) * 2 ** bit;
      }
      return value;
    };
    if (readBits(8) !== 0xc3 || readBits(3) > 3) {
      throw new Error("Clearra returned an unsupported native CTK3 header.");
    }
    const segmentWidth = readBits(5) + 1;
    const countBits = readBits(1) === 0 ? 4 : readBits(1) === 0 ? 8 : readBits(1) === 0 ? 16 : 32;
    const segmentPages = readBits(countBits);
    if (segmentWidth > 31 || segmentPages < 1 || segmentPages > MAX_SEGMENT_PAGES || readBits(1) !== 0) {
      throw new Error("Clearra returned invalid native CTK3 dimensions or extensions.");
    }
    if (width !== null && width !== segmentWidth) {
      throw new Error("Clearra returned inconsistent native CTK3 bundle widths.");
    }
    width = segmentWidth;
    pageCount += segmentPages;
    if (pageCount > MAX_DOCUMENT_PAGES || pageCount > expectedPages) {
      throw new Error("Clearra native CTK3 page count exceeds its receipt or format limit.");
    }
    segments++;
    if (separator < 0) break;
    cursor = separator + 1;
  }
  if (bundled && segments < 2) throw new Error("Clearra returned an invalid native CTK3 bundle.");
  return { pageCount };
}

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
  const info = inspectNativeSolutionEnvelope(source, receipt.solution_count);
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
      inspectNativeSolutionEnvelope(value.source, value.solutionCount).pageCount !== value.solutionCount) {
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
