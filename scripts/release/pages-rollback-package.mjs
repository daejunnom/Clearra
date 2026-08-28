import { createHash } from "node:crypto";
import { readFile, readdir, lstat } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

import { validatePagesIdentity } from "./pages-rollback-authority.mjs";

const BLOCK_SIZE = 512;
const SHA256_PATTERN = /^[0-9a-f]{64}$/u;
const SHA_PATTERN = /^[0-9a-f]{40}$/u;
const UTF8 = new TextDecoder("utf-8", { fatal: true });
const REQUIRED_IDENTITY_PATH = "clearra-build-identity.json";
const REQUIRED_MANIFEST_PATH = "wasm/clearra_wasm.manifest.json";

function fail(message) {
  throw new Error(message);
}

function requirePattern(value, pattern, label) {
  if (typeof value !== "string" || !pattern.test(value)) {
    fail(`${label} has an invalid format`);
  }
  return value;
}

function isZeroBlock(buffer, offset) {
  for (let index = offset; index < offset + BLOCK_SIZE; index += 1) {
    if (buffer[index] !== 0) {
      return false;
    }
  }
  return true;
}

function decodeField(buffer, start, length, label) {
  const end = buffer.indexOf(0, start);
  const boundedEnd = end === -1 || end >= start + length ? start + length : end;
  try {
    return UTF8.decode(buffer.subarray(start, boundedEnd));
  } catch {
    fail(`${label} is not valid UTF-8`);
  }
}

function parseOctalField(buffer, start, length, label) {
  const raw = buffer.subarray(start, start + length);
  if ((raw[0] & 0x80) !== 0) {
    fail(`${label} must use the portable octal tar encoding`);
  }
  const text = Buffer.from(raw)
    .toString("ascii")
    .replace(/\0.*$/u, "")
    .trim();
  if (text === "") {
    return 0;
  }
  if (!/^[0-7]+$/u.test(text)) {
    fail(`${label} is not a valid octal tar field`);
  }
  const value = Number.parseInt(text, 8);
  if (!Number.isSafeInteger(value) || value < 0) {
    fail(`${label} exceeds the supported tar range`);
  }
  return value;
}

function validateHeaderChecksum(buffer, offset) {
  const stored = parseOctalField(buffer, offset + 148, 8, "tar checksum");
  let calculated = 0;
  for (let index = 0; index < BLOCK_SIZE; index += 1) {
    calculated += index >= 148 && index < 156 ? 0x20 : buffer[offset + index];
  }
  if (stored !== calculated) {
    fail("Pages rollback tar header checksum is invalid");
  }
}

function normalizeMemberPath(rawPath, type) {
  if (
    rawPath.length === 0 ||
    rawPath.includes("\\") ||
    rawPath.startsWith("/") ||
    /^[A-Za-z]:/u.test(rawPath) ||
    /[\u0000-\u001f\u007f]/u.test(rawPath)
  ) {
    fail("Pages rollback tar contains an unsafe member path");
  }

  let normalized = rawPath;
  while (normalized.startsWith("./")) {
    normalized = normalized.slice(2);
  }
  normalized = normalized.replace(/\/+$/u, "");
  if (normalized === "" || normalized === ".") {
    if (type !== "5") {
      fail("Pages rollback tar root entry must be a directory");
    }
    return ".";
  }

  const segments = normalized.split("/");
  if (segments.some((segment) => segment === "" || segment === "." || segment === "..")) {
    fail("Pages rollback tar contains an unsafe member path");
  }
  return normalized;
}

export function parseRollbackTar(bufferValue) {
  const buffer = Buffer.isBuffer(bufferValue) ? bufferValue : Buffer.from(bufferValue);
  if (buffer.length < BLOCK_SIZE * 2 || buffer.length % BLOCK_SIZE !== 0) {
    fail("Pages rollback tar must contain complete 512-byte blocks");
  }

  const entries = new Map();
  let offset = 0;
  let foundEnd = false;
  while (offset + BLOCK_SIZE <= buffer.length) {
    if (isZeroBlock(buffer, offset)) {
      if (offset + BLOCK_SIZE * 2 > buffer.length || !isZeroBlock(buffer, offset + BLOCK_SIZE)) {
        fail("Pages rollback tar must end with two zero blocks");
      }
      for (let index = offset; index < buffer.length; index += 1) {
        if (buffer[index] !== 0) {
          fail("Pages rollback tar contains data after its end marker");
        }
      }
      foundEnd = true;
      break;
    }

    validateHeaderChecksum(buffer, offset);
    const name = decodeField(buffer, offset, 100, "tar member name");
    const prefix = decodeField(buffer, offset + 345, 155, "tar member prefix");
    const rawPath = prefix === "" ? name : `${prefix}/${name}`;
    const typeByte = buffer[offset + 156];
    const type = typeByte === 0 ? "0" : String.fromCharCode(typeByte);
    if (type !== "0" && type !== "5") {
      fail("Pages rollback tar contains a link or special entry");
    }
    const path = normalizeMemberPath(rawPath, type);
    if (entries.has(path)) {
      fail("Pages rollback tar contains a duplicate member path");
    }

    const size = parseOctalField(buffer, offset + 124, 12, "tar member size");
    if (type === "5" && size !== 0) {
      fail("Pages rollback tar directory entry must be empty");
    }
    const dataStart = offset + BLOCK_SIZE;
    const dataEnd = dataStart + size;
    if (dataEnd > buffer.length) {
      fail("Pages rollback tar member exceeds the archive boundary");
    }
    entries.set(path, {
      type,
      content: type === "0" ? buffer.subarray(dataStart, dataEnd) : Buffer.alloc(0),
    });
    offset = dataStart + Math.ceil(size / BLOCK_SIZE) * BLOCK_SIZE;
  }

  if (!foundEnd) {
    fail("Pages rollback tar has no valid end marker");
  }
  return entries;
}

function parseRequiredJson(entries, path, label) {
  const entry = entries.get(path);
  if (entry?.type !== "0") {
    fail(`${label} is missing from the Pages rollback tar`);
  }
  try {
    return JSON.parse(UTF8.decode(entry.content));
  } catch {
    fail(`${label} is not valid UTF-8 JSON`);
  }
}

export function validateRollbackPackageBuffer(buffer, { expectedSha, expectedTarSha256 }) {
  const sha = requirePattern(expectedSha, SHA_PATTERN, "snapshot SHA");
  const expectedDigest = requirePattern(
    expectedTarSha256,
    SHA256_PATTERN,
    "captured Pages tar SHA-256",
  );
  const actualDigest = createHash("sha256").update(buffer).digest("hex");
  if (actualDigest !== expectedDigest) {
    fail("Downloaded Pages artifact.tar differs from the captured SHA-256");
  }

  const entries = parseRollbackTar(buffer);
  const identity = parseRequiredJson(entries, REQUIRED_IDENTITY_PATH, "Pages identity");
  const manifest = parseRequiredJson(entries, REQUIRED_MANIFEST_PATH, "Pages WASM manifest");
  validatePagesIdentity(identity, manifest, sha);
  return { actualDigest, entries };
}

export async function validateRollbackPackageDirectory(
  directory,
  { expectedSha, expectedTarSha256 },
) {
  const resolvedDirectory = resolve(directory);
  const names = (await readdir(resolvedDirectory)).sort();
  if (names.length !== 1 || names[0] !== "artifact.tar") {
    fail("Pages rollback download must contain exactly one artifact.tar");
  }
  const tarPath = resolve(resolvedDirectory, "artifact.tar");
  const stat = await lstat(tarPath);
  if (!stat.isFile() || stat.isSymbolicLink()) {
    fail("Pages rollback artifact.tar must be a regular file");
  }
  return validateRollbackPackageBuffer(await readFile(tarPath), {
    expectedSha,
    expectedTarSha256,
  });
}

async function main() {
  const directory = process.env.PAGES_ROLLBACK_PACKAGE_DIR;
  const expectedSha = process.env.SNAPSHOT_SHA;
  const expectedTarSha256 = process.env.SNAPSHOT_TAR_SHA256;
  if (typeof directory !== "string" || directory.length === 0) {
    fail("PAGES_ROLLBACK_PACKAGE_DIR is required");
  }
  await validateRollbackPackageDirectory(directory, { expectedSha, expectedTarSha256 });
  console.log("pages_rollback_package=passed");
}

const invokedPath = process.argv[1] ? resolve(process.argv[1]) : "";
if (invokedPath === resolve(fileURLToPath(import.meta.url))) {
  main().catch((error) => {
    console.error(`pages_rollback_package=failed reason=${error.message}`);
    process.exitCode = 2;
  });
}
